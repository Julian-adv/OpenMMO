import copy
import importlib.util
import subprocess
from functools import cache
from pathlib import Path

import numpy as np
from scipy.spatial import ConvexHull, cKDTree

from lib.glb import read_glb, write_glb

ROOT = Path(__file__).resolve().parents[1]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
spec = importlib.util.spec_from_file_location('plate', ROOT / 'tools/fit-modular-plate.py')
plate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plate)


def with_rig(doc, binary, name):
    base = plate.BASE
    original = [i for i, node in enumerate(base['nodes']) if 'mesh' not in node]
    remap = {old: new for new, old in enumerate(original)}
    doc['nodes'] = [copy.deepcopy(base['nodes'][i]) for i in original]
    for node in doc['nodes']:
        if 'children' in node:
            node['children'] = [remap[i] for i in node['children'] if i in remap]
    skin = copy.deepcopy(base['skins'][0])
    skin['joints'] = [remap[i] for i in skin['joints']]
    if 'skeleton' in skin:
        skin['skeleton'] = remap[skin['skeleton']]
    skin['inverseBindMatrices'] = plate.add_accessor(doc, binary,
        plate.accessor(base, plate.BASE_BIN, base['skins'][0]['inverseBindMatrices']), 'MAT4')
    doc['skins'] = [skin]
    doc['scenes'] = copy.deepcopy(base['scenes'])
    for scene in doc['scenes']:
        scene['nodes'] = [remap[i] for i in scene['nodes'] if i in remap]
    extras = {'rig_id': 'human_male_01_mixamo_candidate_v2', 'part_id': name, 'region': name}
    for index, mesh in enumerate(doc['meshes']):
        metadata = {**extras, **mesh.get('extras', {})}
        mesh_name = mesh.get('name', name) if name == 'pants_barbarian' else name
        mesh.update(name=mesh_name, extras=metadata)
        doc['scenes'][0]['nodes'].append(len(doc['nodes']))
        doc['nodes'].append({'name': mesh_name, 'mesh': index, 'skin': 0, 'extras': metadata})
    doc.pop('animations', None)


def build(name, fit, skinning, mirror=False):
    doc, raw = read_glb(PARTS / 'barbarian_sources' / f'{name}.glb')
    if len(doc['meshes']) != 1:
        raise ValueError(f'{name}: expected a single source mesh')
    binary = bytearray(raw)
    for material in doc['materials']:
        if 'normalTexture' in material:
            material['normalTexture']['scale'] = .5
        material.pop('emissiveTexture', None)
        material.pop('emissiveFactor', None)
    if name == 'pants_barbarian':
        build_pants(doc, raw, binary)
    else:
        for primitive in doc['meshes'][0]['primitives']:
            attrs = primitive['attributes']
            source = plate.accessor(doc, raw, attrs['POSITION'])
            uv = plate.accessor(doc, raw, attrs['TEXCOORD_0'])
            indices = plate.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
            positions = fit(source)
            normals = plate.smooth_normals(positions, indices)
            if mirror:
                positions = np.vstack([positions, positions * [-1, 1, 1]])
                normals = np.vstack([normals, normals * [-1, 1, 1]])
                uv = np.vstack([uv, uv])
                indices = np.vstack([indices, indices[:, ::-1] + len(source)])
            joints, weights = skinning(positions)
            attrs.clear()
            attrs.update(plate.add_skin_attributes(doc, binary, positions, normals, joints, weights))
            attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
            attrs['TANGENT'] = plate.add_accessor(doc, binary,
                plate.tangents(positions, normals, uv, indices), 'VEC4')
            primitive['indices'] = plate.add_accessor(doc, binary, indices.reshape(-1, 1), 'SCALAR', 5125)
    if name in ('boots_barbarian', 'gloves_barbarian'):
        add_cuff(doc, binary, name)
    if name == 'boots_barbarian':
        add_cuff(doc, binary, name, shaft=True)
        add_sandals(doc, binary)
    with_rig(doc, binary, name)
    path = PARTS / 'fitted' / f'{name}.glb'
    write_glb(path, doc, plate.compact(doc, binary))
    triangles = sum(doc['accessors'][p['indices']]['count'] // 3 for mesh in doc['meshes'] for p in mesh['primitives'])
    print(name, 'triangles', triangles)


def top_fit(points):
    result = points * [.36, .60, .29] + [0, 1.51, -.035]
    back = plate.smoothstep((-points[:, 2] - .02) / .35)
    result[:, 1] -= .030 * back
    result[:, 2] -= .005 * back
    surface = nearest_body_surface(result)
    delta = surface - result
    distance = np.linalg.norm(delta, axis=1)
    shoulder = plate.smoothstep((abs(result[:, 0]) - .13) / .08)
    rear = plate.smoothstep((-result[:, 2] - .015) / .07)
    influence = np.maximum(shoulder * .72, rear * .88)
    delta *= (influence * np.maximum(distance - .012, 0) / np.maximum(distance, 1e-6))[:, None]
    nearby = cKDTree(result).query_ball_point(result, .035)
    result += np.array([np.mean(delta[indices], axis=0) for indices in nearby])
    return result


def nearest_body_surface(points):
    triangles = body_triangles(('torso', 'neck', 'upper_arms'))
    near = cKDTree(triangles.mean(1)).query(points, k=24)[1]
    a, b, c = np.moveaxis(triangles[near], 2, 0)
    ab, ac, ap = b - a, c - a, points[:, None] - a
    dot = lambda x, y: np.sum(x * y, axis=-1)
    d00, d01, d11 = dot(ab, ab), dot(ab, ac), dot(ac, ac)
    d20, d21 = dot(ap, ab), dot(ap, ac)
    denom = np.maximum(d00 * d11 - d01 ** 2, 1e-20)
    v, w = (d11 * d20 - d01 * d21) / denom, (d00 * d21 - d01 * d20) / denom
    projected = a + v[..., None] * ab + w[..., None] * ac
    dist = np.where((v >= 0) & (w >= 0) & (v + w <= 1), dot(projected - points[:, None], projected - points[:, None]), np.inf)
    candidates, distances = [projected], [dist]
    for start, end in [(a, b), (b, c), (c, a)]:
        edge = end - start
        t = np.clip(dot(points[:, None] - start, edge) / np.maximum(dot(edge, edge), 1e-20), 0, 1)
        candidate = start + edge * t[..., None]
        candidates.append(candidate)
        distances.append(dot(candidate - points[:, None], candidate - points[:, None]))
    candidates = np.concatenate(candidates, axis=1)
    choice = np.concatenate(distances, axis=1).argmin(1)
    return candidates[np.arange(len(points)), choice]


def pants_fit(points):
    return points * [.37, .25, .44] + [0, .927, -.025]


def clip_height(points, uv, faces, height, above, axis=1):
    vertices, texcoords, triangles = [], [], []
    for face in faces:
        polygon = [(points[i], uv[i]) for i in face]
        clipped = []
        for (a, ta), (b, tb) in zip(polygon, polygon[1:] + polygon[:1]):
            inside_a, inside_b = (a[axis] >= height) == above, (b[axis] >= height) == above
            if inside_a:
                clipped.append((a, ta))
            if inside_a != inside_b:
                t = (height - a[axis]) / (b[axis] - a[axis])
                clipped.append((a + t * (b - a), ta + t * (tb - ta)))
        start = len(vertices)
        vertices.extend(p for p, _ in clipped)
        texcoords.extend(t for _, t in clipped)
        triangles.extend([start, start + i, start + i + 1] for i in range(1, len(clipped) - 1))
    return np.array(vertices), np.array(texcoords), np.array(triangles)


def panel_physics(kind, bone, pivot, outward, length):
    config = {'kind': kind, 'bone': bone, 'pivot': pivot, 'outward': outward, 'length': length,
            'colliders': [
                {'bone': b, 'center': c, 'radii': r}
                for b, c, r in [
                    ('Hips', [0, 1.015, -.023], [.183, .17, .135]),
                    ('LeftUpLeg', [.13, .80, -.012], [.105, .235, .127]),
                    ('RightUpLeg', [-.13, .80, -.012], [.105, .235, .127]),
                    ('LeftLeg', [.165, .33, -.04], [.085, .27, .105]),
                    ('RightLeg', [-.165, .33, -.04], [.085, .27, .105]),
                ]
            ]}
    if kind == 'strap':
        config['colliders'][0].update(center=[0, .98, -.023], radii=[.205, .215, .155])
    return config


def add_panel(doc, binary, name, points, uv, faces, material, physics=None, bone='Hips'):
    normals = plate.smooth_normals(points, faces)
    joints, weights = plate.rigid_weights(len(points), bone)
    attrs = plate.add_skin_attributes(doc, binary, points, normals, joints, weights)
    attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
    if 'normalTexture' in doc['materials'][material]:
        attrs['TANGENT'] = plate.add_accessor(doc, binary, plate.tangents(points, normals, uv, faces), 'VEC4')
    doc['meshes'].append({'name': name, 'extras': {'pelt_physics': physics} if physics else {},
        'primitives': [{'attributes': attrs, 'material': material,
            'indices': plate.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125)}]})


def ray_surface(triangles, origin, direction):
    origin, direction = np.asarray(origin), np.asarray(direction)
    a, b, c = triangles[:, 0], triangles[:, 1], triangles[:, 2]
    e1, e2 = b - a, c - a
    h = np.cross(direction, e2)
    det = np.einsum('ij,ij->i', e1, h)
    inv = np.divide(1., det, out=np.zeros_like(det), where=abs(det) > 1e-10)
    offset = origin - a
    u = np.einsum('ij,ij->i', offset, h) * inv
    q = np.cross(offset, e1)
    v = (q @ direction) * inv
    distance = np.einsum('ij,ij->i', e2, q) * inv
    valid = (abs(det) > 1e-10) & (u >= -1e-6) & (v >= -1e-6) & (u + v <= 1 + 1e-6) & (distance > 0)
    if not valid.any():
        return None
    return distance[valid].max()


def radial_surface(triangles, y, angle):
    return ray_surface(triangles, [0, y, -.025], [np.cos(angle), 0, np.sin(angle)])


@cache
def body_triangles(regions=('legs', 'torso')):
    triangles = []
    for node in plate.BASE['nodes']:
        if node.get('extras', {}).get('region') not in regions:
            continue
        for primitive in plate.BASE['meshes'][node['mesh']]['primitives']:
            points = plate.accessor(plate.BASE, plate.BASE_BIN, primitive['attributes']['POSITION'])
            faces = plate.accessor(plate.BASE, plate.BASE_BIN, primitive['indices']).reshape(-1, 3)
            triangles.extend(points[faces])
    return np.array(triangles)


def fit_belt(points):
    points, inverse = np.unique(points, axis=0, return_inverse=True)
    result = points.copy()
    for i, point in enumerate(points):
        radial = point[[0, 2]] - [0, -.025]
        radius = np.linalg.norm(radial)
        direction = radial / radius
        angle = np.arctan2(direction[1], direction[0])
        surface = radial_surface(body_triangles(), point[1], angle)
        if surface is None:
            raise ValueError(f'Missing waist surface at {point}')
        depth = .160 if direction[1] < 0 else .153
        reference = 1 / np.linalg.norm(direction / [.193, depth])
        result[i, [0, 2]] += direction * min(0, surface + .016 - reference)
        seam = 1 - plate.smoothstep(abs(direction[1]) / .4)
        if seam > 0:
            y = point[1] + seam * (np.clip(point[1], 1.07, 1.14) - point[1])
            surface = radial_surface(body_triangles(), y, angle)
            target = surface + .013 + np.clip(radius - reference, -.004, 0)
            fitted = np.linalg.norm(result[i, [0, 2]] - [0, -.025])
            result[i, [0, 2]] += direction * seam * (target - fitted)
            result[i, 1] = y
    return result[inverse]


def cloth_panel(belt, body, sign, angle, arc, length, columns, thickness, fur):
    rows = 11
    points, uv, faces = [], [], []
    top = 1.11
    for row in range(rows):
        t = row / (rows - 1)
        taper = 1 - (.18 if fur else .20) * plate.smoothstep((t - .7) / .3)
        for col in range(columns):
            across = col / (columns - 1) * 2 - 1
            at = angle + across * arc / 2 * taper
            y = top - length * t + (.03 * across ** 2 * t ** 3 if fur else 0)
            belt_y = y if row < 2 else top - length / (rows - 1)
            radius = radial_surface(belt, belt_y, at if sign > 0 else np.pi - at)
            if radius is None:
                raise ValueError(f'Missing belt attachment at {belt_y:.4f}, {at:.4f}')
            if row < 2:
                radius -= .0015
            else:
                radius += .018 * plate.smoothstep((t - .1) / .5)
                surface = radial_surface(body, y, at if sign > 0 else np.pi - at)
                if surface is not None:
                    radius = max(radius, surface + thickness + .008)
            points.append([sign * radius * np.cos(at), y, -.025 + radius * np.sin(at)])
            uv.append([col / (columns - 1), t])
    points = np.array(points)
    back = points.copy()
    radial = back[:, [0, 2]] - [0, -.025]
    radial /= np.linalg.norm(radial, axis=1, keepdims=True)
    back[:, [0, 2]] -= radial * thickness
    count = len(points)
    for back_face in (False, True):
        offset = count * back_face
        for row in range(rows - 1):
            for col in range(columns - 1):
                a = offset + row * columns + col
                pair = [[a, a + 1, a + columns], [a + 1, a + columns + 1, a + columns]]
                faces.extend([f[::-1] for f in pair] if back_face != (sign < 0) else pair)
    perimeter = (list(range(columns)) + [r * columns + columns - 1 for r in range(1, rows)] +
                 list(range(count - 2, count - columns - 1, -1)) +
                 [r * columns for r in range(rows - 2, 0, -1)])
    for a, b in zip(perimeter, perimeter[1:] + perimeter[:1]):
        pair = [[a, b + count, b], [a, a + count, b + count]]
        faces.extend([f[::-1] for f in pair] if sign < 0 else pair)
    pivot = points[:columns].mean(0).tolist()
    config = panel_physics('fur' if fur else 'strap', 'Hips', pivot,
                           [sign * float(np.cos(angle)), 0, float(np.sin(angle))], length)
    config['cloth'] = {'columns': columns, 'rows': rows, 'pinned_rows': 2}
    return np.vstack([points, back]), np.vstack([uv, uv]), np.array(faces), config


def largest_connected_faces(points, faces):
    from scipy.sparse import coo_matrix
    from scipy.sparse.csgraph import connected_components

    _, weld = np.unique(np.round(points, 5), axis=0, return_inverse=True)
    edges = np.concatenate([weld[faces[:, [0, 1]]], weld[faces[:, [1, 2]]], weld[faces[:, [2, 0]]]])
    graph = coo_matrix((np.ones(len(edges)), (edges[:, 0], edges[:, 1])), shape=(weld.max() + 1,) * 2)
    _, labels = connected_components(graph, directed=False)
    groups = labels[weld[faces[:, 0]]]
    return faces[groups == np.bincount(groups).argmax()]


def pelt_surface(points, uv, faces):
    from shapely import LineString, constrained_delaunay_triangles, set_precision
    from shapely.ops import polygonize, unary_union

    triangles = points[faces]
    xy = triangles[:, :, :2]
    inverse = np.linalg.inv(np.stack([xy[:, 1] - xy[:, 0], xy[:, 2] - xy[:, 0]], axis=2))

    def barycentric(samples):
        vw = np.einsum('fij,sfj->sfi', inverse, samples[:, None] - xy[None, :, 0])
        return np.concatenate([1 - vw.sum(2, keepdims=True), vw], axis=2)

    edges = [set_precision(LineString(triangle[[0, 1, 2, 0]]), 1e-6) for triangle in xy]
    vertices, texcoords = [], []
    for polygon in polygonize(unary_union(edges)):
        if polygon.area < 1e-12:
            continue
        sample = np.array(polygon.representative_point().coords)
        bary = barycentric(sample)[0]
        inside = (bary >= -1e-4).all(1)
        if not inside.any():
            continue
        depth = np.sum(bary * triangles[:, :, 2], axis=1)
        source = np.where(inside, depth, -np.inf).argmax()
        for triangle in constrained_delaunay_triangles(polygon).geoms:
            coords = np.array(triangle.exterior.coords)[:3]
            if not triangle.exterior.is_ccw:
                coords = coords[::-1]
            bary = barycentric(coords)
            inside = (bary >= -1e-3).all(2)
            depth = np.sum(bary * triangles[None, :, :, 2], axis=2)
            z = np.where(inside, depth, -np.inf).max(1)
            if not np.isfinite(z).all():
                raise ValueError('Pelt boundary is outside the source surface')
            vertices.extend(np.column_stack([coords, z]))
            texcoords.extend(bary[:, source] @ uv[faces[source]])
    return np.array(vertices), np.array(texcoords), np.arange(len(vertices)).reshape(-1, 3)


def build_pants(doc, raw, binary):
    source = doc['meshes'][0]['primitives'][0]
    points = pants_fit(plate.accessor(doc, raw, source['attributes']['POSITION']))
    uv = plate.accessor(doc, raw, source['attributes']['TEXCOORD_0'])
    faces = plate.accessor(doc, raw, source['indices']).reshape(-1, 3)
    material = source['material']
    doc['materials'][material]['doubleSided'] = True
    doc['meshes'] = []
    p, t, f = clip_height(points, uv, faces, 1.07, True)
    p, t, f = clip_height(p, t, f, -.025, True, axis=2)
    belt = fit_belt(p), t, f
    add_panel(doc, binary, 'barbarian_belt', *belt, material)
    add_back_belt(doc, binary)
    belt_triangles = []
    for primitive in doc['meshes'][0]['primitives']:
        p = plate.accessor(doc, binary, primitive['attributes']['POSITION'])
        f = plate.accessor(doc, binary, primitive['indices']).reshape(-1, 3)
        belt_triangles.extend(p[f])
    belt_triangles = np.array(belt_triangles)

    centers = points[faces].mean(1)
    front = faces[(centers[:, 1] < 1.055) & (centers[:, 2] > .04)]
    front = largest_connected_faces(points, front)
    p, t, f = clip_height(points, uv, front, 1.055, False)
    cross = np.cross(p[f[:, 1]] - p[f[:, 0]], p[f[:, 2]] - p[f[:, 0]])
    f = largest_connected_faces(p, f[cross[:, 2] > 0])
    p, t, f = pelt_surface(p, t, f)
    p[:, 1] = .69 + (p[:, 1] - .69) * (.39 / .365)
    hanging = np.clip((1.08 - p[:, 1]) / .055, 0, 1)
    depth = (p[:, 2] - .105) * .15 * hanging
    p[:, 2] = .125 + .032 * hanging + depth
    _, weld = np.unique(np.round(p, 5), axis=0, return_inverse=True)
    edges = np.concatenate([f[:, [0, 1]], f[:, [1, 2]], f[:, [2, 0]]])
    _, inverse, counts = np.unique(np.sort(weld[edges], axis=1), axis=0,
                                  return_inverse=True, return_counts=True)
    boundary = edges[counts[inverse] == 1]
    count = len(p)
    sides = [[a, a + count, b] for a, b in boundary] + [[b, a + count, b + count] for a, b in boundary]
    f = np.vstack([f, f[:, ::-1] + count, sides])
    p = np.vstack([p, p - [0, 0, .004]])
    t = np.vstack([t, t])
    for back in (False, True):
        panel = p.copy()
        direction = [0, 0, -1 if back else 1]
        pivot = [0, 1.08, -.183 if back else .125]
        distance = ray_surface(belt_triangles, [0, pivot[1], -.025], direction)
        anchor_z = -.025 + direction[2] * (distance - .0015)
        if back:
            panel[:, 2] = anchor_z - front_profile
        else:
            panel[:, 2] += (anchor_z - pivot[2]) * (1 - plate.smoothstep((1.08 - panel[:, 1]) / .16))
            front_profile = panel[:, 2] - anchor_z
        pivot[2] = anchor_z
        physics = panel_physics('plate', 'Hips', pivot, direction, .39)
        if back:
            physics['colliders'][0].update(center=[0, .985, 0], radii=[.25, .155, .145])
        add_panel(doc, binary, 'barbarian_pelt_' + ('back' if back else 'front'), panel, t,
                  f[:, ::-1] if back else f, material, physics)

    materials = {}
    for kind in ('fur', 'leather'):
        materials[kind] = len(doc['materials'])
        doc['materials'].append({'name': 'Barbarian ' + kind,
            'pbrMetallicRoughness': {'baseColorTexture': {'index': reference_texture(doc, binary, 'barbarian_' + kind)},
                                    'metallicFactor': 0, 'roughnessFactor': .94}})
    for sign, side in [(1, 'left'), (-1, 'right')]:
        p, uv, f, config = cloth_panel(belt_triangles, body_triangles(), sign,
                                      .12, 1.22, .33, 9, .006, True)
        add_panel(doc, binary, 'barbarian_pelt_' + side, p, uv, f, materials['fur'], config)
        for i, (angle, length) in enumerate([(.83, .28), (.69, .28)]):
            p, uv, f, config = cloth_panel(belt_triangles, body_triangles(), sign,
                                          angle, .115, length, 3, .003, False)
            add_panel(doc, binary, f'barbarian_strap_{side}_{i}', p, uv, f,
                      materials['leather'], config)


def helmet_fit(points):
    return points * [.23, .18, .21] + [0, 1.843, -.006]


def boots_fit(points):
    y = points[:, 1] * .19 + .295
    source = source_points('boots_barbarian')
    source_center, source_radius = cross_sections(source[:, 1], source[:, [0, 2]], points[:, 1], .10)
    center, radius = leg_sections(y)
    local = (points[:, [0, 2]] - source_center) / source_radius
    length = np.linalg.norm(local, axis=1, keepdims=True)
    local *= np.maximum(length, 1.1) / np.maximum(length, 1e-5)
    cross = center + local * (radius + .012)
    return np.column_stack([cross[:, 0], y, cross[:, 1]])


@cache
def source_points(name):
    doc, raw = read_glb(PARTS / 'barbarian_sources' / f'{name}.glb')
    return np.concatenate([plate.accessor(doc, raw, p['attributes']['POSITION'])
                           for p in doc['meshes'][0]['primitives']])


def cross_sections(axis, cross, values, band):
    samples = np.linspace(axis.min(), axis.max(), 30)
    low, high = [], []
    for sample in samples:
        points = cross[abs(axis - sample) < band]
        if not len(points):
            points = cross[np.argsort(abs(axis - sample))[:8]]
        low.append(points.min(0))
        high.append(points.max(0))
    center = (np.array(low) + high) / 2
    radius = np.maximum((np.array(high) - low) / 2, .005)
    return tuple(np.column_stack([np.interp(values, samples, data[:, i]) for i in range(2)])
                 for data in (center, radius))


def leg_sections(y):
    body = plate.BODY_POS[(plate.BODY_POS[:, 0] > .06) &
                          (plate.BODY_POS[:, 1] < .56)]
    return cross_sections(body[:, 1], body[:, [0, 2]], y, .025)


def arm_sections(t):
    elbow, wrist = plate.BONES['LeftForeArm'], plate.BONES['LeftHand']
    axis = plate.unit(wrist - elbow)
    width = plate.unit(np.cross(np.array([0, 0, 1.]), axis))
    body = plate.BODY_POS[plate.BODY_POS[:, 0] > .25] - elbow
    along = body @ axis / np.linalg.norm(wrist - elbow)
    return cross_sections(along, np.column_stack([body @ width, body[:, 2]]), t, .07)


def gloves_fit(points):
    elbow, wrist = plate.BONES['LeftForeArm'], plate.BONES['LeftHand']
    axis = plate.unit(wrist - elbow)
    front = np.array([0, 0, 1.])
    width = plate.unit(np.cross(front, axis))
    t = (.95 - points[:, 1]) / 1.90 * .71 + .22
    center, radius = arm_sections(t)
    depth = center[:, 1] + radius[:, 1] + .008
    return (elbow + (wrist - elbow) * t[:, None] +
            (depth + points[:, 2] * .065)[:, None] * front +
            (center[:, 0] + points[:, 0] / .465 * (radius[:, 0] + .01))[:, None] * width)


def paired_rigid_weights(points, bone):
    joints, weights = plate.rigid_weights(len(points), 'Left' + bone)
    right, _ = plate.rigid_weights(len(points), 'Right' + bone)
    joints[points[:, 0] < 0] = right[points[:, 0] < 0]
    return joints, weights


def reference_texture(doc, binary, name):
    for i, image in enumerate(doc['images']):
        if image.get('name') == name:
            return next(j for j, texture in enumerate(doc['textures']) if texture.get('source') == i)
    image_path = ROOT / f'doc/images/characters/modular_human_male_01/parts/{name}.png'
    data = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(image_path),
        '-vf', 'scale=1024:1024:flags=lanczos', '-f', 'image2pipe', '-vcodec', 'png', '-'])
    binary.extend(b'\0' * (-len(binary) % 4))
    view = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(data)})
    binary.extend(data)
    image = len(doc['images'])
    doc['images'].append({'bufferView': view, 'mimeType': 'image/png', 'name': name})
    texture = len(doc['textures'])
    doc['textures'].append({'source': image})
    return texture


def add_sandals(doc, binary):
    texture = reference_texture(doc, binary, 'barbarian_leather')
    material = len(doc['materials'])
    doc['materials'].append({'name': 'Leather sandal', 'doubleSided': True,
        'pbrMetallicRoughness': {'baseColorTexture': {'index': texture},
                               'metallicFactor': 0, 'roughnessFactor': .9}})
    foot = plate.BASE['meshes'][next(node['mesh'] for node in plate.BASE['nodes']
                                  if node.get('extras', {}).get('region') == 'feet')]
    primitive = foot['primitives'][0]
    body = plate.accessor(plate.BASE, plate.BASE_BIN, primitive['attributes']['POSITION'])
    indices = plate.accessor(plate.BASE, plate.BASE_BIN, primitive['indices']).reshape(-1, 3)
    triangles = body[indices]
    triangles = triangles[(triangles[:, :, 0] > 0).all(1)]
    footprint = body[(body[:, 0] > 0) & (body[:, 1] < .075)][:, [0, 2]]
    outline = footprint[ConvexHull(footprint).vertices]
    center = outline.mean(0)
    outline += plate.unit(outline - center) * .005
    points, uv, faces = [], [], []
    count = len(outline)
    for y in [-.009, .002]:
        points.extend([[x, y, z] for x, z in outline])
        uv.extend((outline - center) * 4 + .5)
    for i in range(count):
        j = (i + 1) % count
        faces.extend([[i, j, i + count], [j, j + count, i + count]])
    for i in range(1, count - 1):
        faces.extend([[0, i + 1, i], [count, count + i, count + i + 1]])
    faces = [face[::-1] for face in faces]
    for z, width in [(.038, .041), (.108, .038)]:
        start = len(points)
        segments = 18
        for row in range(3):
            depth = z + (row / 2 - .5) * width
            near = footprint[abs(footprint[:, 1] - depth) < .028]
            lo, hi = near[:, 0].min() - .004, near[:, 0].max() + .004
            origin = np.array([(lo + hi) / 2, .018, depth])
            for col in range(segments + 1):
                t = col / segments
                angle = np.pi * (1 - t)
                direction = np.array([np.cos(angle), np.sin(angle), 0])
                radius = ray_surface(triangles, origin, direction)
                if radius is None:
                    raise ValueError(f'Missing instep surface at {depth}, {angle}')
                point = origin + direction * (radius + .006)
                if col in (0, segments):
                    point[1] = .003
                points.append(point)
                uv.append([t * .65, row / 2 * .20 + z * 2])
        for row in range(2):
            for col in range(segments):
                a = start + row * (segments + 1) + col
                b = a + segments + 1
                faces.extend([[a, b, a + 1], [a + 1, b, b + 1]])
    start = len(points)
    segments = 28
    for row in range(3):
        for col in range(segments + 1):
            t = col / segments
            angle = -np.pi * t
            y = .046 + .016 * np.sin(np.pi * t) + (row / 2 - .5) * .023
            origin = np.array([plate.BONES['LeftFoot'][0], y, .023])
            direction = np.array([np.cos(angle), 0, np.sin(angle)])
            radius = ray_surface(triangles, origin, direction)
            if radius is None:
                raise ValueError(f'Missing heel surface at {y}, {angle}')
            points.append(origin + direction * (radius + .006))
            uv.append([t * 1.4, row / 2 * .13 + .5])
    for row in range(2):
        for col in range(segments):
            a = start + row * (segments + 1) + col
            b = a + segments + 1
            faces.extend([[a, a + 1, b], [a + 1, b + 1, b]])
    points, uv, faces = np.array(points), np.array(uv), np.array(faces)
    normals = plate.smooth_normals(points, faces)
    faces = np.vstack([faces, faces[:, ::-1] + len(points)])
    points = np.vstack([points, points * [-1, 1, 1]])
    normals = np.vstack([normals, normals * [-1, 1, 1]])
    joints, weights = plate.transfer_weights(points)
    attrs = plate.add_skin_attributes(doc, binary, points, normals, joints, weights)
    attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, np.vstack([uv, uv]), 'VEC2')
    doc['meshes'][0]['primitives'].append({'attributes': attrs, 'material': material,
        'indices': plate.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125)})


def add_cuff(doc, binary, name, shaft=False):
    fur = name == 'boots_barbarian' and not shaft
    texture = reference_texture(doc, binary, 'barbarian_fur' if fur else 'barbarian_leather')
    material = len(doc['materials'])
    pbr = {'metallicFactor': 0, 'roughnessFactor': .95, 'baseColorTexture': {'index': texture}}
    doc['materials'].append({'name': 'Tawny fur cuff' if fur else 'Leather bracer sleeve',
                              'pbrMetallicRoughness': pbr, 'doubleSided': True})
    points, uv = [], []
    count = 32 if fur else 20
    rows = 5 if fur or shaft else 9
    for row in range(rows):
        for i in range(count + 1):
            angle = i / count * 2 * np.pi
            if fur or shaft:
                y = ([.398, .43, .478, .482, .405] if fur else [.13, .20, .29, .38, .44])[row]
                padding = [.020, .033, .026, .010, .010][row] if fur else .009
                if fur and row == 0:
                    y += .008 * np.cos(angle * 11)
                    padding += .005 * np.cos(angle * 11)
                center, radius = leg_sections(np.array([y]))
                cross = center[0] + [np.cos(angle), np.sin(angle)] * (radius[0] + padding)
                point = np.array([cross[0], y, cross[1]])
                tex = [i / count * 3, row / 4 * .7 if fur else (.44 - y) * 5]
            else:
                elbow, wrist = plate.BONES['LeftForeArm'], plate.BONES['LeftHand']
                axis = plate.unit(wrist - elbow)
                width = plate.unit(np.cross(np.array([0, 0, 1.]), axis))
                t = [.20, .22, .35, .50, .65, .80, .92, .94, .94][row]
                center, radius = arm_sections(np.array([t]))
                padding = [.012, .011, .011, .011, .011, .013, .015, .016, .008][row]
                cross = center[0] + [np.cos(angle), np.sin(angle)] * (radius[0] + padding)
                point = elbow + (wrist - elbow) * t + width * cross[0]
                point += np.array([0, 0, 1.]) * cross[1]
                tex = [i / count * 2, t * 1.5]
            points.append(point)
            uv.append(tex)
    faces = []
    for row in range(rows - 1):
        for i in range(count):
            a, b = row * (count + 1) + i, (row + 1) * (count + 1) + i
            faces.extend([[a, b, a + 1], [a + 1, b, b + 1]])
    points, faces, uv = np.array(points), np.array(faces), np.array(uv)
    if not fur and not shaft:
        faces = faces[:, ::-1]
    normals = plate.smooth_normals(points, faces)
    faces = np.vstack([faces, faces[:, ::-1] + len(points)])
    points = np.vstack([points, points * [-1, 1, 1]])
    normals = np.vstack([normals, normals * [-1, 1, 1]])
    uv = np.vstack([uv, uv])
    joints, weights = paired_rigid_weights(points, 'Leg' if fur or shaft else 'ForeArm')
    attrs = plate.add_skin_attributes(doc, binary, points, normals, joints, weights)
    attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
    doc['meshes'][0]['primitives'].append({'attributes': attrs,
        'indices': plate.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125),
        'material': material})


def add_back_belt(doc, binary):
    texture = reference_texture(doc, binary, 'barbarian_leather')
    material = len(doc['materials'])
    doc['materials'].append({'name': 'Brown leather rear belt',
        'pbrMetallicRoughness': {'baseColorTexture': {'index': texture},
                                'metallicFactor': 0, 'roughnessFactor': .92}})
    points, faces, uv = [], [], []
    count = 24
    for row in range(4):
        y = [1.065, 1.14, 1.14, 1.065][row]
        inset = .004 if row > 1 else 0
        for i in range(count + 1):
            angle = np.pi - .035 + i / count * (np.pi + .07)
            radius = radial_surface(body_triangles(), y, angle) + .013 - inset
            points.append([np.cos(angle) * radius, y, -.025 + np.sin(angle) * radius])
            uv.append([i / count * 3, (1.14 - y) * 5])
    for row in range(3):
        for i in range(count):
            a, b = row * (count + 1) + i, (row + 1) * (count + 1) + i
            faces.extend([[a, b, a + 1], [a + 1, b, b + 1]])
    points, faces = np.array(points), np.array(faces)
    normals = plate.smooth_normals(points, faces)
    joints, weights = plate.rigid_weights(len(points), 'Hips')
    attrs = plate.add_skin_attributes(doc, binary, points, normals, joints, weights)
    attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
    doc['meshes'][0]['primitives'].append({
        'attributes': attrs,
        'indices': plate.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125),
        'material': material,
    })


if __name__ == '__main__':
    build('helmet_barbarian', helmet_fit, lambda p: plate.rigid_weights(len(p), 'Head'))
    build('top_barbarian', top_fit, plate.transfer_weights)
    build('pants_barbarian', pants_fit, None)
    build('boots_barbarian', boots_fit, lambda p: paired_rigid_weights(p, 'Leg'), mirror=True)
    build('gloves_barbarian', gloves_fit, lambda p: paired_rigid_weights(p, 'ForeArm'), mirror=True)
