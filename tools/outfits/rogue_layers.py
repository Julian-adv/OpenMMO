"""Continuous rogue vest, scarf and wrist wrap on the canonical rig."""
from collections import Counter

import numpy as np
from scipy.interpolate import PchipInterpolator

ATLAS = 'assets/modular_human_male_01/parts/rogue_rebuild_v5/material-atlas.png'
TORSO_PROFILE = np.array([
    [1.105, .177, .133, .119],
    [1.13, .174, .129, .123],
    [1.19, .168, .126, .128],
    [1.25, .174, .129, .145],
    [1.31, .195, .142, .169],
    [1.35, .215, .153, .188],
    [1.395, .235, .157, .200],
    [1.44, .249, .140, .200],
    [1.53, .262, .095, .185],
    [1.595, .268, .065, .170],
])
TORSO_CURVE = PchipInterpolator(TORSO_PROFILE[:, 0], TORSO_PROFILE[:, 1:], axis=0)
SECTION_EXPONENT = .75


def torso_profile(height):
    values = TORSO_CURVE(np.clip(height, TORSO_PROFILE[0, 0], TORSO_PROFILE[-1, 0]))
    return values[..., 0], values[..., 1], values[..., 2]


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
    return compact_weights(dense)


def compact_weights(dense):
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(axis=1, keepdims=True)


def garment_weights(fit, points, shoulders):
    points = np.asarray(points)
    joints, weights = torso_weights(fit, points)
    dense = np.zeros((len(points), len(fit.NAMES)))
    np.add.at(dense, (np.arange(len(points))[:, None], joints), weights)
    for side, root in shoulders.items():
        edges = np.roll(root, -1, axis=0) - root
        along = np.clip(np.sum((points[:, None] - root) * edges, axis=2) / np.sum(edges * edges, axis=1), 0, 1)
        distance = np.linalg.norm(points[:, None] - root - along[..., None] * edges, axis=2).min(axis=1)
        amount = 1 - fit.io.smoothstep((distance - .020) / .14)
        dense *= 1 - amount[:, None]
        dense[:, fit.NAMES.index(side + 'Shoulder')] += .65 * amount
        dense[:, fit.NAMES.index(side + 'Arm')] += .35 * amount
    return compact_weights(dense)


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


def clip_garment(points, faces, uv, distance):
    points, uv = np.asarray(points), np.asarray(uv)
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
            if distance[a] * distance[b] < 0:
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
    levels = TORSO_PROFILE[:, 0]
    points, uv = [], []
    for row, height in enumerate(levels):
        rx, front, back = torso_profile(height)
        z = np.where(np.cos(theta) >= 0, front, back) * np.sign(np.cos(theta)) * abs(np.cos(theta)) ** SECTION_EXPONENT - .015
        y = height - .085 * fit.io.smoothstep((height - 1.38) / .215) * np.maximum(0, np.cos(theta)) ** 8
        y += (.006 * np.sin(theta) ** 2 - .010 * np.maximum(0, np.cos(theta)) ** 4) * (1 - fit.io.smoothstep((height - 1.105) / .085))
        points.extend(np.column_stack([rx * np.sin(theta), y, z]))
        uv.extend(np.column_stack([theta / (2 * np.pi), np.full(n, 1 - row / len(levels))]))
    points.extend(np.column_stack([.106 * np.sin(theta), np.full(n, 1.63), -.03 + np.where(np.cos(theta) >= 0, .08, .09) * np.cos(theta)]))
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
    shoulders = {('Left' if points[loop, 0].mean() > 0 else 'Right'): points[loop] for loop in loops if len(loop) != n}
    hem_height = 1.485 - .080 * np.maximum(0, -np.cos(theta)) ** 4 + .12 * abs(np.sin(theta)) ** 4
    hem_height = np.where(abs(np.sin(theta)) > .82, 1.595, np.minimum(hem_height, 1.595))
    hem_row = np.interp(hem_height, np.append(levels, 1.63), np.arange(len(levels) + 1))
    distance = np.repeat(np.arange(len(levels) + 1), n) - np.tile(hem_row, len(levels) + 1)
    distance[abs(distance) < 1e-10] = 0
    vest_points, vest_faces, vest_uv = clip_garment(points, faces, uv, distance)
    result = emit(fit, doc, binary, 'vest_rogue', vest_points, vest_faces, atlas_uv(vest_uv, 'leather'), mats['leather'], garment_weights(fit, vest_points, shoulders))
    shirt_points = points.copy()
    shirt_points[:, 0] *= .94
    shirt_points[:, 2] = -.025 + (shirt_points[:, 2] + .025) * .94
    shirt_faces = faces[(faces >= 4 * n).all(axis=1)][:, [1, 2, 0]]
    shirt_skin = garment_weights(fit, shirt_points, shoulders)
    emit(fit, doc, binary, 'shirt_armholes', shirt_points, shirt_faces, atlas_uv(uv, 'linen'), mats['linen'], shirt_skin)
    roots = {}
    for loop in loops:
        if len(loop) != n:
            root = shirt_points[loop]
            side = 'Left' if root[:, 0].mean() > 0 else 'Right'
            roots[side] = (root, (shirt_skin[0][loop], shirt_skin[1][loop]))
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
        if len(loop) == n:
            emit(fit, doc, binary, f'vest_bound_edge_{index}', rim_points, rim_faces, atlas_uv(rim_uv, 'trim'), mats['trim'])
        else:
            rim_points[len(loop):] = shirt_points[loop]
            rim_distance = np.tile(distance[loop], 2)
            for name, signed_distance, material in [('leather', rim_distance, mats['leather']), ('scarf', -rim_distance, mats['scarf'])]:
                binding_points, binding_faces, binding_uv = clip_garment(rim_points, rim_faces, rim_uv, signed_distance)
                emit(fit, doc, binary, f'armhole_binding_{index}_{name}', binding_points, binding_faces, atlas_uv(binding_uv, name), material, garment_weights(fit, binding_points, shoulders))
    result.update(expected_openings=['waist', 'connected scarf and armhole edge'], construction_opening_loops=len(loops), hem_y_m=1.105,
                  scarf_connection='Shared garment cut boundary and skin weights',
                  silhouette=dict(profile_columns=['height_m', 'half_width_m', 'front_radius_m', 'back_radius_m'],
                                  profile=TORSO_PROFILE.tolist(), interpolation='PCHIP', section_exponent=SECTION_EXPONENT,
                                  waist_to_chest_width_ratio=float(torso_profile(1.19)[0] / torso_profile(1.395)[0]),
                                  hem_front_drop_m=.010, hem_side_raise_m=.006))
    return result, roots, (points, faces, uv, distance, shoulders)


def scarf(fit, doc, binary, mats, shell):
    n = 40
    theta = np.linspace(0, 2 * np.pi, n + 1)
    points, uv = [], []
    profiles = [(.081, .073, 1.676), (.092, .083, 1.668), (.105, .091, 1.647), (.098, .085, 1.638)]
    for row, (rx, rz, y) in enumerate(profiles):
        amount = row / 7
        radius = 1 + .025 * np.sin(5 * theta + row * .9) * amount
        ring = np.column_stack([rx * np.sin(theta) * radius,
            y + .0015 * np.sin(3 * theta) * amount,
            -.043 + rz * np.cos(theta) * radius])
        points.extend(ring)
        uv.extend(np.column_stack([theta / (2 * np.pi), np.full(n + 1, amount)]))
    shell_points, shell_faces, shell_uv, distance, shoulders = shell
    neck = shell_points[-n:]
    points.extend(np.vstack([neck, neck[:1]]))
    uv.extend(np.column_stack([theta / (2 * np.pi), np.full(n + 1, 1.2)]))
    faces = grid_faces(len(profiles) + 1, n + 1)[:, ::-1]
    drape_points, drape_faces, drape_uv = clip_garment(shell_points, shell_faces, shell_uv, -distance)
    faces = np.vstack([faces, drape_faces + len(points)])
    points, uv = np.vstack([points, drape_points]), np.vstack([uv, drape_uv])
    result = emit(fit, doc, binary, 'scarf_rogue', points, faces, atlas_uv(uv, 'scarf'), mats['scarf'], garment_weights(fit, points, shoulders))
    result['expected_openings'] = ['neckline', 'continuous outer drape edge']
    result['garment_attachment'] = dict(clearance_m=0, method='Continuous shoulder drape on the vest shell',
                                       skinning='Exact shared cut-edge positions and torso weights')
    return result


def front_z(x, y):
    radius, depth, _ = torso_profile(y)
    return -.015 + depth * np.maximum(0, 1 - (x / radius) ** 2) ** (SECTION_EXPONENT / 2)


def fasteners(fit, doc, binary, mats, shoulders):
    points, uv = [], []
    rows = 12
    for t in np.linspace(0, 1, rows):
        x, y = -.08 + .17 * t, 1.12 + .33 * t
        for side in [-1, 1]:
            px, py = x + side * .018, y - side * .009
            points.append([px, py, front_z(px, py) + .004])
            uv.append([(side + 1) / 2, t])
    emit(fit, doc, binary, 'vest_overlap_strap', points, grid_faces(rows, 2), atlas_uv(uv, 'trim'), mats['trim'], garment_weights(fit, points, shoulders))
    for index, t in enumerate([.12, .40, .70, .91]):
        x, y = -.08 + .17 * t, 1.12 + .33 * t
        z = front_z(x, y) + .009
        angles = np.arange(12) * 2 * np.pi / 12
        points = [[x, y, z + .002]] + np.column_stack([x + .008 * np.cos(angles), y + .008 * np.sin(angles), np.full(12, z)]).tolist()
        faces = [[0, i + 1, (i + 1) % 12 + 1] for i in range(12)]
        uv = [[.5, .5]] + np.column_stack([.5 + .5 * np.cos(angles), .5 + .5 * np.sin(angles)]).tolist()
        emit(fit, doc, binary, f'vest_button_{index}', points, faces, uv, mats['brass'], garment_weights(fit, points, shoulders))


def build_top(fit, doc, binary):
    from outfits.rogue_sleeves import add_sleeves
    mats = materials(fit, doc, binary)
    vest_report, roots, shell = vest(fit, doc, binary, mats)
    report = dict(vest=vest_report)
    fasteners(fit, doc, binary, mats, shell[-1])
    report['sleeves'] = add_sleeves(fit, doc, binary, mats['linen'], roots)
    report['scarf'] = scarf(fit, doc, binary, mats, shell)
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
