"""Continuous rogue vest, scarf and wrist wrap on the canonical rig."""
from collections import Counter

import numpy as np

ATLAS = 'assets/modular_human_male_01/parts/rogue_rebuild_v5/material-atlas.png'


def materials(fit, doc, binary):
    result = {}
    for name, roughness in [('leather', .78), ('scarf', .95), ('linen', .95), ('trim', .85)]:
        binary.extend(b'\0' * (-len(binary) % 4))
        raw = (fit.ROOT / ATLAS).with_name(name + '.png').read_bytes()
        view = len(doc.setdefault('bufferViews', []))
        doc['bufferViews'].append(dict(buffer=0, byteOffset=len(binary), byteLength=len(raw)))
        binary.extend(raw)
        image = len(doc.setdefault('images', []))
        doc['images'].append(dict(bufferView=view, mimeType='image/png'))
        texture = len(doc.setdefault('textures', []))
        doc['textures'].append(dict(source=image))
        result[name] = len(doc.setdefault('materials', []))
        doc['materials'].append(dict(name='rogue_' + name, doubleSided=True,
            pbrMetallicRoughness=dict(baseColorTexture=dict(index=texture), metallicFactor=0, roughnessFactor=roughness)))
    result['brass'] = len(doc['materials'])
    doc['materials'].append(dict(name='aged_brass', doubleSided=True,
        pbrMetallicRoughness=dict(baseColorFactor=[.32, .20, .07, 1], metallicFactor=.65, roughnessFactor=.43)))
    return result


def atlas_uv(uv, quadrant):
    scale = {'leather': [3, 2], 'scarf': [4, 2], 'linen': [3, 2], 'trim': [2, 2]}[quadrant]
    return np.asarray(uv) * scale


def torso_weights(fit, points):
    y = points[:, 1]
    levels = [1.10, 1.23, 1.38, 1.50]
    dense = np.zeros((len(points), len(fit.NAMES)))
    for index, name in enumerate(['Hips', 'Spine', 'Spine1', 'Spine2']):
        values = np.zeros(4)
        values[index] = 1
        dense[:, fit.NAMES.index(name)] = np.interp(y, levels, values)
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        amount = .50 * fit.io.smoothstep((points[:, 0] * sign - .08) / .06) * fit.io.smoothstep((y - 1.37) / .06)
        dense *= 1 - amount[:, None]
        dense[:, fit.NAMES.index(side + 'Shoulder')] += amount
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(axis=1, keepdims=True)


def emit(fit, doc, binary, name, points, faces, uv, material, skin=None):
    io = fit.io
    points, faces, uv = np.asarray(points), np.asarray(faces), np.asarray(uv)
    used, inverse = np.unique(faces, return_inverse=True)
    points, uv, faces = points[used], uv[used], inverse.reshape(-1, 3)
    normals = io.smooth_normals(points, faces)
    joints, weights = torso_weights(fit, points) if skin is None else (skin[0][used], skin[1][used])
    attrs = io.add_skin_attributes(doc, binary, points, normals, joints, weights)
    attrs['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    attrs['TANGENT'] = io.add_accessor(doc, binary, io.tangents(points, normals, uv, faces), 'VEC4')
    doc['meshes'].append(dict(name=name, primitives=[dict(attributes=attrs, material=material,
        indices=io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125))]))
    _, welded = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    edges = np.sort(np.concatenate([welded[faces[:, [0, 1]]], welded[faces[:, [1, 2]]], welded[faces[:, [2, 0]]]]), axis=1)
    _, counts = np.unique(edges, axis=0, return_counts=True)
    assert counts.max() <= 2, name
    return dict(mesh=name, triangles=len(faces), boundary_edges=int((counts == 1).sum()), nonmanifold_edges=0)


def grid_faces(rows, columns):
    return np.array([[a, a + 1, b] if k == 0 else [a + 1, b + 1, b]
                     for row in range(rows - 1) for col in range(columns - 1)
                     for a, b in [(row * columns + col, (row + 1) * columns + col)] for k in range(2)])


def edge_loops(faces):
    counts = Counter(tuple(sorted(edge)) for face in faces for edge in zip(face, np.roll(face, -1)))
    neighbors = {}
    for (a, b), count in counts.items():
        if count == 1:
            neighbors.setdefault(a, []).append(b)
            neighbors.setdefault(b, []).append(a)
    assert all(len(adjacent) == 2 for adjacent in neighbors.values())
    loops = []
    while neighbors:
        start = next(iter(neighbors))
        loop, previous, current = [], None, start
        while True:
            loop.append(current)
            nxt = next(vertex for vertex in neighbors[current] if vertex != previous)
            previous, current = current, nxt
            if current == start:
                break
        for index in loop:
            del neighbors[index]
        loops.append(loop)
    return loops


def scarf_height(theta, base_height, amount=1):
    return (base_height - .12 * np.maximum(0, -np.cos(theta)) ** 4 * amount ** 3
            - .045 * np.maximum(0, np.cos(theta)) * amount
            + .10 * abs(np.sin(theta)) ** 3 * amount ** 2
            + .012 * np.sin(theta) * amount ** 2)


def clip_under_scarf(points, faces, uv):
    points, uv = np.asarray(points), np.asarray(uv)
    angles = np.arctan2(points[:, 0] / .275, (points[:, 2] + .043) / .165)
    distance = points[:, 1] - scarf_height(angles, 1.525) - .012
    output, coords, triangles = [], [], []
    for face in faces:
        face_uv = uv[face].copy()
        if np.ptp(face_uv[:, 0]) > .5:
            face_uv[face_uv[:, 0] < .5, 0] += 1
        polygon = []
        for corner, (a, b) in enumerate(zip(face, np.roll(face, -1))):
            ta, tb = face_uv[corner], face_uv[(corner + 1) % 3]
            if distance[a] <= 0:
                polygon.append((points[a], ta))
            if (distance[a] < 0) != (distance[b] < 0):
                t = distance[a] / (distance[a] - distance[b])
                polygon.append((points[a] + t * (points[b] - points[a]), ta + t * (tb - ta)))
        start = len(output)
        output.extend(p[0] for p in polygon)
        coords.extend(p[1] for p in polygon)
        triangles.extend([[start, start + i, start + i + 1] for i in range(1, len(polygon) - 1)])
    return np.array(output), np.array(triangles), np.array(coords)


def vest(fit, doc, binary, mats):
    n = 40
    theta = np.arange(n) * 2 * np.pi / n
    levels = np.array([1.105, 1.13, 1.19, 1.25, 1.31, 1.35, 1.395, 1.44, 1.53, 1.595])
    points, uv = [], []
    for row, height in enumerate(levels):
        rx = np.interp(height, [1.105, 1.25, 1.395, 1.595], [.176, .184, .235, .268])
        front = np.interp(height, [1.105, 1.31, 1.44, 1.595], [.140, .150, .140, .065])
        back = np.interp(height, [1.105, 1.31, 1.44, 1.595], [.126, .185, .200, .17])
        z = np.where(np.cos(theta) >= 0, front, back) * np.sign(np.cos(theta)) * np.sqrt(abs(np.cos(theta))) - .015
        y = height - .085 * fit.io.smoothstep((height - 1.38) / .215) * np.maximum(0, np.cos(theta)) ** 8
        points.extend(np.column_stack([rx * np.sin(theta), y, z]))
        uv.extend(np.column_stack([theta / (2 * np.pi), np.full(n, 1 - row / len(levels))]))
    points.extend(np.column_stack([.106 * np.sin(theta), 1.63 - .13 * np.maximum(0, np.cos(theta)) ** 8, -.03 + np.where(np.cos(theta) >= 0, .12, .09) * np.cos(theta)]))
    uv.extend(np.column_stack([theta / (2 * np.pi), np.zeros(n)]))
    faces = []
    for row in range(len(levels)):
        for col in range(n):
            mid = (col + .5) * 2 * np.pi / n
            if 5 <= row <= 8 and abs(np.sin(mid)) > .82:
                continue
            a, b = row * n + col, (row + 1) * n + col
            an, bn = row * n + (col + 1) % n, (row + 1) * n + (col + 1) % n
            faces.extend([[a, an, b], [an, bn, b]])
    points, faces, uv = np.array(points), np.array(faces), np.array(uv)
    loops = edge_loops(faces)
    assert len(loops) == 4
    for loop in loops:
        if len(loop) != n:
            for _ in range(3):
                p = points[loop]
                points[loop] = .5 * p + .25 * np.roll(p, 1, axis=0) + .25 * np.roll(p, -1, axis=0)
    angles = np.arctan2(points[:, 0] / .275, (points[:, 2] + .043) / .165)
    covered = points[:, 1] > scarf_height(angles, 1.525) + .012
    hidden_faces = int(np.count_nonzero(covered[faces].all(axis=1)))
    clipped_points, clipped_faces, clipped_uv = clip_under_scarf(points, faces, uv)
    result = emit(fit, doc, binary, 'vest_rogue', clipped_points, clipped_faces, atlas_uv(clipped_uv, 'leather'), mats['leather'])
    shirt_points = points.copy()
    shirt_points[:, 0] *= .94
    shirt_points[:, 2] = -.025 + (shirt_points[:, 2] + .025) * .94
    shirt_faces = []
    for row in range(4, len(levels)):
        for col in range(n):
            if abs(np.sin((col + .5) * 2 * np.pi / n)) < .65:
                continue
            a, b = row * n + col, (row + 1) * n + col
            an, bn = row * n + (col + 1) % n, (row + 1) * n + (col + 1) % n
            shirt_faces.extend([[a, b, an], [an, b, bn]])
    shirt_faces = np.array(shirt_faces)
    shirt_faces = shirt_faces[~covered[shirt_faces].all(axis=1)]
    clipped_points, clipped_faces, clipped_uv = clip_under_scarf(shirt_points, shirt_faces[:, ::-1], uv)
    emit(fit, doc, binary, 'shirt_armholes', clipped_points, clipped_faces, atlas_uv(clipped_uv, 'linen'), mats['linen'])
    for index, loop in enumerate(loops):
        if points[loop, 1].min() > 1.48:
            continue
        edge = points[loop]
        normal = fit.io.unit(np.column_stack([edge[:, 0], np.zeros(len(edge)), edge[:, 2] + .025]))
        inner = edge - normal * .004
        rim_points = np.vstack([edge, inner])
        rim_faces = []
        for col in range(len(loop)):
            nxt = (col + 1) % len(loop)
            rim_faces.extend([[col, nxt, col + len(loop)], [nxt, nxt + len(loop), col + len(loop)]])
        rim_uv = np.column_stack([np.tile(np.linspace(0, 1, len(loop)), 2), np.repeat([0, .05], len(loop))])
        rim_points, rim_faces, rim_uv = clip_under_scarf(rim_points, rim_faces, rim_uv)
        emit(fit, doc, binary, f'vest_bound_edge_{index}', rim_points, rim_faces, atlas_uv(rim_uv, 'trim'), mats['trim'])
    result.update(expected_openings=['waist', 'neck under scarf', 'left arm', 'right arm'], construction_opening_loops=len(loops), hem_y_m=1.105,
                  scarf_overlap_m=.012, hidden_under_scarf_triangles=hidden_faces)
    return result


def scarf(fit, doc, binary, mats):
    n = 40
    theta = np.linspace(0, 2 * np.pi, n + 1)
    points, uv = [], []
    profiles = [(.081, .073, 1.636), (.092, .083, 1.628), (.105, .091, 1.607),
                (.098, .085, 1.590), (.142, .110, 1.569), (.170, .13, 1.552),
                (.240, .151, 1.512), (.275, .165, 1.485)]
    for row, (rx, rz, y) in enumerate(profiles):
        amount = row / (len(profiles) - 1)
        radius = 1 + .025 * np.sin(5 * theta + row * .9) * amount
        ring = np.column_stack([rx * np.sin(theta) * radius,
            scarf_height(theta, y + .04, amount),
            -.043 + rz * np.cos(theta) * radius])
        xradius = np.interp(ring[:, 1], [1.105, 1.25, 1.395, 1.595, 1.63], [.176, .184, .235, .268, .106])
        zradius = np.where(np.cos(theta) > 0,
            np.interp(ring[:, 1], [1.105, 1.31, 1.44, 1.595], [.140, .150, .140, .065]),
            np.interp(ring[:, 1], [1.105, 1.31, 1.44, 1.595, 1.63], [.126, .185, .200, .17, .09]))
        distance = np.sqrt((ring[:, 0] / xradius) ** 2 + ((ring[:, 2] + .015) / zradius) ** 2)
        expansion = np.where(ring[:, 1] < 1.646, np.maximum(1, 1.08 / distance), 1)
        ring[:, 0] *= expansion
        ring[:, 2] = -.015 + (ring[:, 2] + .015) * expansion
        points.extend(ring)
        uv.extend(np.column_stack([theta / (2 * np.pi), np.full(n + 1, amount)]))
    points, uv = np.array(points), np.array(uv)
    faces = grid_faces(len(profiles), n + 1)[:, ::-1]
    result = emit(fit, doc, binary, 'scarf_rogue', points, faces, atlas_uv(uv, 'scarf'), mats['scarf'])
    result['expected_openings'] = ['neckline', 'continuous outer drape edge']
    return result


def front_z(x, y):
    radius = np.interp(y, [1.105, 1.25, 1.395, 1.595], [.176, .184, .235, .268])
    depth = np.interp(y, [1.105, 1.31, 1.44, 1.595], [.140, .150, .140, .065])
    return -.015 + depth * np.maximum(0, 1 - (x / radius) ** 2) ** .25


def fasteners(fit, doc, binary, mats):
    points, uv = [], []
    rows = 12
    for t in np.linspace(0, 1, rows):
        x, y = -.08 + .17 * t, 1.12 + .33 * t
        for side in [-1, 1]:
            px, py = x + side * .018, y - side * .009
            points.append([px, py, front_z(px, py) + .004])
            uv.append([(side + 1) / 2, t])
    emit(fit, doc, binary, 'vest_overlap_strap', points, grid_faces(rows, 2), atlas_uv(uv, 'trim'), mats['trim'])
    for index, t in enumerate([.12, .40, .70, .91]):
        x, y = -.08 + .17 * t, 1.12 + .33 * t
        z = front_z(x, y) + .009
        angles = np.arange(12) * 2 * np.pi / 12
        points = [[x, y, z + .002]] + np.column_stack([x + .008 * np.cos(angles), y + .008 * np.sin(angles), np.full(12, z)]).tolist()
        faces = [[0, i + 1, (i + 1) % 12 + 1] for i in range(12)]
        uv = [[.5, .5]] + np.column_stack([.5 + .5 * np.cos(angles), .5 + .5 * np.sin(angles)]).tolist()
        emit(fit, doc, binary, f'vest_button_{index}', points, faces, uv, mats['brass'])


def build_top(fit, doc, binary):
    from outfits.rogue_sleeves import add_sleeves
    mats = materials(fit, doc, binary)
    report = dict(vest=vest(fit, doc, binary, mats), scarf=scarf(fit, doc, binary, mats))
    fasteners(fit, doc, binary, mats)
    report['sleeves'] = add_sleeves(fit, doc, binary, mats['linen'])
    report['texture'] = dict(path=ATLAS, sha256=fit.digest(fit.ROOT / ATLAS))
    return report


def wrist_section(triangles, origin, axis, basis, radial, mid=None):
    distance = (triangles - origin) @ axis
    crossing = (distance.min(axis=1) < 0) & (distance.max(axis=1) > 0)
    segments = []
    for tri, d in zip(triangles[crossing], distance[crossing]):
        hits = []
        for a, b in [(0, 1), (1, 2), (2, 0)]:
            if d[a] * d[b] < 0:
                t = d[a] / (d[a] - d[b])
                hits.append((tri[a] + t * (tri[b] - tri[a]) - origin) @ basis.T)
        if len(hits) == 2:
            segments.append(hits)
    segments = np.array(segments)
    if mid is None:
        mid = (segments.reshape(-1, 2).min(axis=0) + segments.reshape(-1, 2).max(axis=0)) / 2
    start, edge = segments[:, 0] - mid, segments[:, 1] - segments[:, 0]
    cross = lambda a, b: a[..., 0] * b[..., 1] - a[..., 1] * b[..., 0]
    denominator = cross(radial[:, None], edge)
    along = np.divide(cross(start, edge)[None, :], denominator, out=np.full_like(denominator, -np.inf), where=abs(denominator) > 1e-10)
    between = np.divide(cross(start, radial[:, None]), denominator, out=np.full_like(denominator, -np.inf), where=abs(denominator) > 1e-10)
    radius = np.where((between >= 0) & (between <= 1) & (along > 0), along, -np.inf).max(axis=1)
    assert np.isfinite(radius).all(), 'Open wrist body section'
    return mid, radius


def build_wrap(fit, doc, binary):
    mats = materials(fit, doc, binary)
    io = fit.io
    wrist, elbow = fit.BONES['LeftHand'], fit.BONES['LeftForeArm']
    axis = io.unit(elbow - wrist)
    across = io.unit(np.cross(axis, [0, 0, 1]))
    forward = np.cross(axis, across)
    basis = np.array([across, forward])
    skin, faces, dense = fit.body_surface(('hands', 'forearms'), 1)
    n = 32
    theta = np.linspace(0, 2 * np.pi, n + 1)
    radial = np.column_stack([np.cos(theta), np.sin(theta)])
    heights = np.linspace(.012, .077, 9)
    points, uv = [], []
    for row, height in enumerate(heights):
        origin = wrist + axis * height
        mid, radius = wrist_section(skin[faces], origin, axis, basis, radial)
        ridge = .001 * np.cos(2 * np.pi * (height / .016 - theta / (2 * np.pi)))
        points.extend(origin + (mid + radial * (radius + .014 + ridge)[:, None]) @ basis)
        uv.extend(np.column_stack([theta / (2 * np.pi), np.full(n + 1, row / (len(heights) - 1))]))
    points = np.array(points)
    joints, weights = fit.transfer(points, (skin, faces, dense), {'LeftArm', 'LeftForeArm', 'LeftHand', 'LeftHandThumb1', 'LeftHandThumb2'})
    result = emit(fit, doc, binary, 'wrap_rogue_left', points, grid_faces(len(heights), n + 1),
                  atlas_uv(uv, 'linen'), mats['linen'], (joints, weights))
    result.update(minimum_radial_clearance_m=.013, axial_interval_m=[.012, .077], closed_circumference=True,
                  method='Exact wrist body sections with local reference skin weights including thumb-root influence')
    return result
