import copy
import importlib.util
import json
import subprocess
import tempfile
from pathlib import Path

import numpy as np
from scipy.interpolate import PchipInterpolator
from scipy.ndimage import gaussian_filter1d, distance_transform_edt
from scipy.spatial import cKDTree
from lib.glb import view_bytes

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/face_tripo_rugged_v1'
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io
HEAD = fit.NAMES.index('Head')


def dense_weights(doc, raw, primitive):
    attrs = primitive['attributes']
    joints = io.accessor(doc, raw, attrs['JOINTS_0'])
    weights = io.accessor(doc, raw, attrs['WEIGHTS_0'])
    dense = np.zeros((len(joints), len(fit.NAMES)))
    np.add.at(dense, (np.arange(len(joints))[:, None], joints), weights)
    return dense


def skin(dense):
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(1, keepdims=True)


def transition_normals(points, faces):
    triangles = points[faces]
    cross = np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0])
    centers = triangles.mean(1)
    distance, neighbors = cKDTree(centers).query(points, k=128)
    kernel = np.exp(-.5 * (distance / .010) ** 2)
    return io.unit(np.sum(cross[neighbors] * kernel[..., None], axis=1))


def clip(points, uv, normals, faces, scores=None):
    if scores is None:
        heights = 1.706 - .04 * io.smoothstep((points[:, 2] + .01) / .06) + np.maximum(np.abs(points[:, 0]) - .065, 0) * .4
        scores = points[:, 1] - heights
    scores = list(scores)
    points, uv, normals = map(list, (points, uv, normals))
    intersections, result = {}, []
    for face in faces:
        polygon = []
        for a, b in zip(face, np.roll(face, -1)):
            if scores[a] >= 0:
                polygon.append(a)
            if (scores[a] < 0) != (scores[b] < 0):
                key = tuple(sorted((int(a), int(b))))
                if key not in intersections:
                    t = -scores[a] / (scores[b] - scores[a])
                    intersections[key] = len(points)
                    points.append(points[a] * (1 - t) + points[b] * t)
                    uv.append(uv[a] * (1 - t) + uv[b] * t)
                    normals.append(normals[a] * (1 - t) + normals[b] * t)
                polygon.append(intersections[key])
        for i in range(1, len(polygon) - 1):
            result.append([polygon[0], polygon[i], polygon[i + 1]])
    faces = np.array(result)
    used, inverse = np.unique(faces, return_inverse=True)
    return np.array(points)[used], np.array(uv)[used], io.unit(np.array(normals)[used]), inverse.reshape(-1, 3)


def boundary_ring(points, faces, upper=False):
    _, first, welded = np.unique(np.round(points, 6), axis=0, return_index=True, return_inverse=True)
    welded_faces = welded[faces]
    edges = np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]], welded_faces[:, [2, 0]]]), axis=1)
    edges, counts = np.unique(edges, axis=0, return_counts=True)
    edges = edges[counts == 1]
    links = {}
    for a, b in edges:
        links.setdefault(a, []).append(b)
        links.setdefault(b, []).append(a)
    assert all(len(values) == 2 for values in links.values())
    unseen, rings = set(links), []
    while unseen:
        start = next(iter(unseen))
        ring, previous, current = [], None, start
        while current != start or not ring:
            ring.append(current)
            following = next(value for value in links[current] if value != previous)
            previous, current = current, following
        unseen.difference_update(ring)
        rings.append(first[ring])
    ring = (max if upper else min)(rings, key=lambda ids: points[ids, 1].min())
    contour = points[ring]
    area = np.sum(contour[:, 2] * np.roll(contour[:, 0], -1) - contour[:, 0] * np.roll(contour[:, 2], -1))
    if area < 0:
        ring = ring[::-1]
    return np.roll(ring, -np.argmin(points[ring, 2]))


def parameters(points):
    lengths = np.linalg.norm(np.roll(points[:, [0, 2]], -1, axis=0) - points[:, [0, 2]], axis=1)
    return np.concatenate([[0], np.cumsum(lengths)]) / lengths.sum()


def sample(values, fractions, targets):
    return np.column_stack([np.interp(targets, fractions, np.append(values[:, i], values[0, i])) for i in range(values.shape[1])])


def canonical_neck():
    node = next(n for n in io.BASE['nodes'] if n.get('extras', {}).get('region') == 'head')
    primitive = io.BASE['meshes'][node['mesh']]['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(io.BASE, io.BASE_BIN, attrs['POSITION'])
    faces = io.accessor(io.BASE, io.BASE_BIN, primitive['indices']).reshape(-1, 3)
    uv = io.accessor(io.BASE, io.BASE_BIN, attrs['TEXCOORD_0'])
    normals = io.accessor(io.BASE, io.BASE_BIN, attrs['NORMAL'])
    dense = dense_weights(io.BASE, io.BASE_BIN, primitive)
    base_ring = boundary_ring(points, faces)
    bottom = points[base_ring].copy()
    donor_surface = (points.copy(), faces, uv)
    segments = np.roll(bottom, -1, axis=0) - bottom
    offset = points[:, None, [0, 2]] - bottom[None, :, [0, 2]]
    fraction = np.clip(np.einsum('nbi,bi->nb', offset, segments[:, [0, 2]]) /
        np.sum(segments[:, [0, 2]] ** 2, axis=1), 0, 1)
    closest = bottom[None, :, [0, 2]] + segments[None, :, [0, 2]] * fraction[:, :, None]
    selected = np.argmin(np.linalg.norm(points[:, None, [0, 2]] - closest, axis=-1), axis=1)
    height = bottom[selected, 1] + segments[selected, 1] * fraction[np.arange(len(points)), selected] + .004
    points, uv, normals, faces = clip(points, uv, normals, faces, height - points[:, 1])
    _, _, weights = fit.nearest_surface(points, (io.accessor(io.BASE, io.BASE_BIN, attrs['POSITION']),
        io.accessor(io.BASE, io.BASE_BIN, primitive['indices']).reshape(-1, 3), dense))
    offset = points[:, None, [0, 2]] - bottom[None, :, [0, 2]]
    fraction = np.clip(np.einsum('nbi,bi->nb', offset, segments[:, [0, 2]]) /
        np.sum(segments[:, [0, 2]] ** 2, axis=1), 0, 1)
    closest = bottom[None] + segments[None] * fraction[:, :, None]
    selected = np.argmin(np.linalg.norm(points[:, None, [0, 2]] - closest[:, :, [0, 2]], axis=-1), axis=1)
    front = points[:, 2] > .015
    points[front, 2] = np.minimum(points[front, 2], closest[np.arange(len(points)), selected, 2][front])
    geometry = dict(points=points, uv=uv, normals=normals, faces=faces, dense=weights)
    return geometry, bottom, primitive['material'], donor_surface


def collar(source_points, source_uv, source_normals, source_faces, source_dense, lower_geometry):
    body_points, body_uv, body_normals, body_faces, body_dense = [lower_geometry[k] for k in ['points', 'uv', 'normals', 'faces', 'dense']]
    lower = boundary_ring(body_points, body_faces, upper=True)
    upper = boundary_ring(source_points, source_faces)
    lower_t, upper_t = parameters(body_points[lower]), parameters(source_points[upper])
    fractions = np.unique(np.concatenate([lower_t, upper_t]))
    fractions = fractions[np.append(True, np.diff(fractions) > 1e-8)]
    fractions[-1] = 1
    lo = [sample(a[lower], lower_t, fractions) for a in [body_points, body_uv, body_normals, body_dense]]
    hi = [sample(a[upper], upper_t, fractions) for a in [source_points, source_uv, source_normals, source_dense]]
    neck_node = next(n for n in io.BASE['nodes'] if n.get('extras', {}).get('region') == 'neck')
    neck_primitive = io.BASE['meshes'][neck_node['mesh']]['primitives'][0]
    neck_attrs = neck_primitive['attributes']
    neck_surface = tuple(io.accessor(io.BASE, io.BASE_BIN, neck_attrs[key]) for key in ['POSITION', 'TEXCOORD_0'])
    neck_faces = io.accessor(io.BASE, io.BASE_BIN, neck_primitive['indices']).reshape(-1, 3)
    _, _, lo[1] = fit.nearest_surface(lo[0] - [0, .012, 0], (neck_surface[0], neck_faces, neck_surface[1]))
    _, _, hi[1] = fit.nearest_surface(hi[0] + [0, .035, 0], (source_points, source_faces, source_uv))
    points, normals, weights, atlas, body_tex, head_tex, colors, faces = [], [], [], [], [], [], [], []
    row_fractions = [lower_t, fractions, fractions, fractions, fractions, fractions, fractions, upper_t]
    starts = []
    for row, (t, targets) in enumerate(zip(np.linspace(0, 1, len(row_fractions)), row_fractions)):
        starts.append(len(points))
        lower_values = [np.column_stack([np.interp(targets, fractions, value[:, i]) for i in range(value.shape[1])]) for value in lo]
        upper_values = [np.column_stack([np.interp(targets, fractions, value[:, i]) for i in range(value.shape[1])]) for value in hi]
        blend = t * t * (3 - 2 * t)
        contour = lower_values[0] * (1 - t) + upper_values[0] * t
        if 0 < t < 1:
            uniform = np.linspace(0, 1, 256, endpoint=False)
            samples = np.column_stack([np.interp(uniform, targets, contour[:, i]) for i in range(3)])
            smoothed = gaussian_filter1d(samples, 3, axis=0, mode='wrap')
            smooth_contour = sample(smoothed, np.append(uniform, 1), targets)
            contour = contour * (1 - np.sin(np.pi * t)) + smooth_contour * np.sin(np.pi * t)
        points.extend(contour)
        normals.extend(lower_values[2] * (1 - t) + upper_values[2] * t)
        weights.extend(lower_values[3] * (1 - t) + upper_values[3] * t)
        atlas.extend(np.column_stack([.02 + targets * .96, np.full(len(targets), .98 - t * .96)]))
        body_tex.extend(lower_values[1])
        head_tex.extend(upper_values[1])
        colors.extend(np.tile([1, 1, 1, blend], (len(targets), 1)))
        if row:
            previous = row_fractions[row - 1]
            i = j = 0
            while i < len(previous) - 1 or j < len(targets) - 1:
                a, b = starts[row - 1] + i, starts[row] + j
                left = previous[i + 1] if i + 1 < len(previous) else 2
                right = targets[j + 1] if j + 1 < len(targets) else 2
                if abs(left - right) < 1e-8:
                    faces.extend([[a, a + 1, b + 1], [a, b + 1, b]])
                    i, j = i + 1, j + 1
                elif left < right:
                    faces.append([a, a + 1, b])
                    i += 1
                else:
                    faces.append([a, b + 1, b])
                    j += 1
    faces = np.array(faces)
    triangles = np.array(points, dtype=np.float32)[faces]
    valid = np.linalg.norm(np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]), axis=1) > 1e-12
    faces = faces[valid]
    return dict(points=np.array(points), normals=io.unit(np.array(normals)), dense=np.array(weights),
        uv=np.array(atlas), body_uv=np.array(body_tex), head_uv=np.array(head_tex), colors=np.array(colors), faces=np.array(faces)), dict(
        lower_vertices=len(lower), upper_vertices=len(upper), lower_row_count=len(lower_t), upper_row_start=starts[-1],
        lower_y_range_m=[float(body_points[lower, 1].min()), float(body_points[lower, 1].max())],
        top_y_range_m=[float(source_points[upper, 1].min()), float(source_points[upper, 1].max())],
        exact_lower_position_and_weight_interpolation=True, exact_upper_position_and_weight_interpolation=True)


def face_document(source, raw, geometry, neck, lower_neck, lower_material, material_kind):
    doc, binary = copy.deepcopy(source), bytearray(raw)
    body_material = fit.append_source(doc, binary, io.BASE, io.BASE_BIN) + lower_material
    if material_kind == 'work':
        material = body_material - lower_material
        node = next(n for n in io.BASE['nodes'] if n.get('extras', {}).get('region') == 'head')
        material += io.BASE['meshes'][node['mesh']]['primitives'][0]['material']
    else:
        content = (OUTPUT / 'neck-transition.png').read_bytes()
        binary.extend(b'\0' * (-len(binary) % 4))
        view = len(doc['bufferViews'])
        doc['bufferViews'].append(dict(buffer=0, byteOffset=len(binary), byteLength=len(content)))
        binary.extend(content)
        image = len(doc['images'])
        doc['images'].append(dict(bufferView=view, mimeType='image/png'))
        texture = len(doc['textures'])
        doc['textures'].append(dict(source=image))
        material = len(doc['materials'])
        doc['materials'].append(dict(name='neck_transition', doubleSided=True,
            pbrMetallicRoughness=dict(baseColorTexture=dict(index=texture), metallicFactor=0, roughnessFactor=.9)))
    lower_skin = copy.deepcopy(doc['materials'][body_material])
    lower_skin['name'] = 'retained_neck_skin'
    lower_skin.pop('normalTexture', None)
    lower_skin['pbrMetallicRoughness'].pop('metallicRoughnessTexture', None)
    lower_skin['pbrMetallicRoughness'].update(metallicFactor=0, roughnessFactor=.9)
    body_material = len(doc['materials'])
    doc['materials'].append(lower_skin)
    meshes = []
    for name, data, index in [('face_rugged', geometry, 0), ('face_neck_bridge', neck, material), ('face_neck_lower', lower_neck, body_material)]:
        joints, weights = skin(data['dense'])
        attrs = io.add_skin_attributes(doc, binary, data['points'], data['normals'], joints, weights)
        attrs['TEXCOORD_0'] = io.add_accessor(doc, binary, data['uv'], 'VEC2')
        if material_kind == 'work' and name == 'face_neck_bridge':
            for number, field in [(1, 'body_uv'), (2, 'head_uv')]:
                attrs[f'TEXCOORD_{number}'] = io.add_accessor(doc, binary, data[field], 'VEC2')
            attrs['COLOR_0'] = io.add_accessor(doc, binary, data['colors'], 'VEC4')
        meshes.append(dict(name=name, primitives=[dict(attributes=attrs, material=index,
            indices=io.add_accessor(doc, binary, data['faces'].reshape(-1, 1), 'SCALAR', 5125))]))
    doc['meshes'] = meshes
    fit.with_rig(doc, binary, 'face_rugged', 'rugged_face_v1')
    return doc, io.compact(doc, binary)


def assemble(face, raw, neck_normals):
    doc, binary = copy.deepcopy(io.BASE), bytearray(io.BASE_BIN)
    node = next(n for n in doc['nodes'] if n.get('extras', {}).get('region') == 'neck')
    primitive = doc['meshes'][node['mesh']]['primitives'][0]
    primitive['attributes']['NORMAL'] = io.add_accessor(doc, binary, neck_normals, 'VEC3')
    material = copy.deepcopy(doc['materials'][primitive['material']])
    material['name'] = 'rugged_body_neck_skin'
    material.pop('normalTexture', None)
    material['pbrMetallicRoughness'].pop('metallicRoughnessTexture', None)
    material['pbrMetallicRoughness'].update(metallicFactor=0, roughnessFactor=.9)
    primitive['material'] = len(doc['materials'])
    doc['materials'].append(material)
    remove = {n['mesh'] for n in doc['nodes'] if n.get('extras', {}).get('region') == 'head'}
    kept = [m for i, m in enumerate(doc['meshes']) if i not in remove]
    material_offset = fit.append_source(doc, binary, face, raw)
    for mesh in face['meshes']:
        mesh = copy.deepcopy(mesh)
        for primitive in mesh['primitives']:
            attrs = primitive['attributes']
            for key, index in list(attrs.items()):
                a = face['accessors'][index]
                attrs[key] = io.add_accessor(doc, binary, io.accessor(face, raw, index), a['type'], a['componentType'])
            index = primitive['indices']
            primitive['indices'] = io.add_accessor(doc, binary, io.accessor(face, raw, index), 'SCALAR', 5125)
            primitive['material'] += material_offset
        kept.append(mesh)
    doc['meshes'] = kept
    fit.with_rig(doc, binary, 'base', 'rugged_face_v1')
    for node in doc['nodes']:
        if node.get('name') == 'face_rugged':
            node['extras']['region'] = 'head'
            node['extras']['face_triangle_count'] = face['accessors'][face['meshes'][0]['primitives'][0]['indices']]['count'] // 3
        elif node.get('name') in ['face_neck_bridge', 'face_neck_lower']:
            node['extras']['region'] = 'face_neck_bridge'
        elif 'mesh' in node:
            original = next(n for n in io.BASE['nodes'] if 'mesh' in n and io.BASE['meshes'][n['mesh']]['name'] == node['name'])
            node['extras'] = copy.deepcopy(original['extras'])
            node['name'] = original['name']
        if 'mesh' in node:
            doc['meshes'][node['mesh']]['extras'] = copy.deepcopy(node['extras'])
    return doc, io.compact(doc, binary)


def fit_hair(surface):
    path = io.PARTS / 'hair_tripo_wavy_v1/hair_wavy_bone.glb'
    doc, raw = fit.read_glb(path)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(doc, raw, attrs['POSITION'])
    canonical = fit.body_surface(('head',))
    old, _, _ = fit.nearest_surface(points, canonical)
    new, normals, _ = fit.nearest_surface(old, surface)
    distance = np.linalg.norm(points - old, axis=1)
    influence = io.smoothstep((points[:, 1] - 1.70) / .08) * (1 - io.smoothstep((distance - .045) / .025))
    result = points + (new - old) * influence[:, None]
    q, n, _ = fit.nearest_surface(result, surface)
    signed = np.einsum('ij,ij->i', result - q, n)
    near = np.linalg.norm(result - q, axis=1)
    mask = (result[:, 1] > 1.74) & (near < .025) & (signed < .004)
    result[mask] = q[mask] + n[mask] * .006
    binary = bytearray(raw)
    attrs['POSITION'] = io.add_accessor(doc, binary, result, 'VEC3')
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    attrs['NORMAL'] = io.add_accessor(doc, binary, io.smooth_normals(result, faces), 'VEC3')
    fit.write_glb(OUTPUT / 'hair_wavy_bone_rugged.glb', doc, io.compact(doc, binary))
    return dict(source_sha256=fit.digest(path), maximum_shift_mm=float(np.max(np.linalg.norm(result - points, axis=1)) * 1000), triangles=len(faces))


def validate_body_preservation(doc, raw):
    for original in io.BASE['nodes']:
        if 'mesh' not in original or original.get('extras', {}).get('region') == 'head':
            continue
        candidate = next(n for n in doc['nodes'] if n.get('name') == original['name'])
        before = io.BASE['meshes'][original['mesh']]['primitives']
        after = doc['meshes'][candidate['mesh']]['primitives']
        assert len(before) == len(after)
        for a, b in zip(before, after):
            for key in a['attributes']:
                if key == 'NORMAL' and original.get('extras', {}).get('region') == 'neck':
                    points = io.accessor(io.BASE, io.BASE_BIN, a['attributes']['POSITION'])
                    mask = points[:, 1] <= 1.59
                    assert np.array_equal(io.accessor(io.BASE, io.BASE_BIN, a['attributes'][key])[mask], io.accessor(doc, raw, b['attributes'][key])[mask])
                    continue
                assert np.array_equal(io.accessor(io.BASE, io.BASE_BIN, a['attributes'][key]), io.accessor(doc, raw, b['attributes'][key]))
            assert np.array_equal(io.accessor(io.BASE, io.BASE_BIN, a['indices']), io.accessor(doc, raw, b['indices']))
    return True


def neck_projection(neck, geometry, body_surface, path):
    size = 512
    positions = np.zeros((size, size, 3))
    mask = np.zeros((size, size), dtype=bool)
    atlas = neck['uv'] * [1, -1] + [0, 1]
    for face in neck['faces']:
        uv = atlas[face] * (size - 1)
        low = np.maximum(np.floor(uv.min(0)).astype(int), 0)
        high = np.minimum(np.ceil(uv.max(0)).astype(int), size - 1)
        x, y = np.meshgrid(np.arange(low[0], high[0] + 1), np.arange(low[1], high[1] + 1))
        inverse = np.linalg.inv(np.column_stack([uv[1] - uv[0], uv[2] - uv[0]]))
        bary = (np.stack([x, y], axis=-1) - uv[0]) @ inverse.T
        bary = np.concatenate([1 - bary.sum(-1, keepdims=True), bary], axis=-1)
        inside = np.min(bary, axis=-1) >= -1e-6
        positions[y[inside], x[inside]] = bary[inside] @ neck['points'][face]
        mask[y[inside], x[inside]] = True
    _, nearest = distance_transform_edt(~mask, return_indices=True)
    positions = positions[nearest[0], nearest[1]].reshape(-1, 3)
    source_surface = (geometry['points'], geometry['faces'], geometry['uv'])
    body_uv, head_uv = [], []
    neck_node = next(n for n in io.BASE['nodes'] if n.get('extras', {}).get('region') == 'neck')
    primitive = io.BASE['meshes'][neck_node['mesh']]['primitives'][0]
    attrs = primitive['attributes']
    skin_surface = (io.accessor(io.BASE, io.BASE_BIN, attrs['POSITION']),
        io.accessor(io.BASE, io.BASE_BIN, primitive['indices']).reshape(-1, 3),
        io.accessor(io.BASE, io.BASE_BIN, attrs['TEXCOORD_0']))
    donor_uv = []
    for chunk in np.array_split(positions, 64):
        body_uv.extend(fit.nearest_surface(chunk, body_surface)[2])
        donor_uv.extend(fit.nearest_surface(chunk - [0, .055, 0], skin_surface)[2])
        head_uv.extend(fit.nearest_surface(chunk + [0, .001, 0], source_surface)[2])
    row = np.indices(mask.shape)[0][nearest[0], nearest[1]] / (size - 1)
    t = np.clip((row - .02) / .96, 0, 1)
    blend = t * t * (3 - 2 * t)
    np.savez_compressed(path, body_uv=np.array(body_uv).reshape(size, size, 2),
        head_uv=np.array(head_uv).reshape(size, size, 2), donor_uv=np.array(donor_uv).reshape(size, size, 2), blend=blend)


def main():
    source_path = OUTPUT / 'source.glb'
    assert fit.digest(source_path) == '939d8c700ba5a33ae3818bc8b2c89947080ab94b7a9f4634ed19daafe8f36d3f'
    source, raw = fit.read_glb(source_path)
    primitive = source['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    original = io.accessor(source, raw, attrs['POSITION'])
    uv = io.accessor(source, raw, attrs['TEXCOORD_0'])
    normals = io.accessor(source, raw, attrs['NORMAL'])
    faces = io.accessor(source, raw, primitive['indices']).reshape(-1, 3)
    curve = PchipInterpolator([.18, .65265, .99951171875], [1.663, 1.7825, 1.9])
    points = original * [.30, 1, .29] + [-.0593262 * .30, 0, .010]
    points[:, 1] = curve(original[:, 1])
    normals = io.unit(normals / np.column_stack([np.full(len(points), .30), curve.derivative()(original[:, 1]), np.full(len(points), .29)]))
    points, uv, normals, faces = clip(points, uv, normals, faces)
    _, _, dense = fit.nearest_surface(points, fit.body_surface(('head', 'neck')))
    rigid = ((points[:, 2] > .045) & (points[:, 1] > 1.666)) | (points[:, 1] > 1.72)
    dense[rigid] = 0
    dense[rigid, HEAD] = 1
    geometry = dict(points=points, uv=uv, normals=normals, dense=dense, faces=faces)
    lower_neck, body_seam, lower_material, donor_surface = canonical_neck()
    neck, seam = collar(points, uv, normals, faces, dense, lower_neck)
    combined_points = np.concatenate([points, neck['points'], lower_neck['points']])
    combined_faces = np.concatenate([faces, neck['faces'] + len(points), lower_neck['faces'] + len(points) + len(neck['points'])])
    node = next(n for n in io.BASE['nodes'] if n.get('extras', {}).get('region') == 'neck')
    body_primitive = io.BASE['meshes'][node['mesh']]['primitives'][0]
    body_points = io.accessor(io.BASE, io.BASE_BIN, body_primitive['attributes']['POSITION'])
    body_faces = io.accessor(io.BASE, io.BASE_BIN, body_primitive['indices']).reshape(-1, 3)
    body_normals = io.accessor(io.BASE, io.BASE_BIN, body_primitive['attributes']['NORMAL'])
    smoothed = transition_normals(np.concatenate([combined_points, body_points]),
        np.concatenate([combined_faces, body_faces + len(combined_points)]))
    combined_normals = smoothed[:len(combined_points)]
    head_seam = points[boundary_ring(points, faces)]
    distance_to_cut = cKDTree(head_seam).query(points)[0]
    smoothing = 1 - io.smoothstep(distance_to_cut / .024)
    geometry['normals'] = io.unit(geometry['normals'] * (1 - smoothing[:, None]) + combined_normals[:len(points)] * smoothing[:, None])
    neck['normals'] = combined_normals[len(points):len(points) + len(neck['points'])]
    lower_neck['normals'] = combined_normals[len(points) + len(neck['points']):]
    amount = io.smoothstep((body_points[:, 1] - 1.59) / .03)
    modified = amount > 0
    body_normals[modified] = io.unit(body_normals[modified] * (1 - amount[modified, None]) + smoothed[len(combined_points):][modified] * amount[modified, None])
    with tempfile.TemporaryDirectory(prefix='rugged-face-bake-') as directory:
        work_path = Path(directory) / 'face-work.glb'
        projection = Path(directory) / 'neck-projection.npz'
        neck_projection(neck, geometry, donor_surface, projection)
        doc, binary = face_document(source, raw, geometry, neck, lower_neck, lower_material, 'work')
        fit.write_glb(work_path, doc, binary)
        subprocess.run(['blender', '-b', '--python-exit-code', '1', '--python', str(ROOT / 'tools/blender-scripts/bake_tripo_face_neck.py'), '--', str(work_path), str(OUTPUT / 'neck-transition.png'), str(projection)], check=True)
    doc, binary = face_document(source, raw, geometry, neck, lower_neck, lower_material, 'final')
    assert view_bytes(source, raw, source['images'][0]['bufferView']) == view_bytes(doc, binary, doc['images'][0]['bufferView'])
    fit.write_glb(OUTPUT / 'face_rugged.glb', doc, binary)
    assembled, assembled_raw = assemble(doc, binary, body_normals)
    body_preserved = validate_body_preservation(assembled, assembled_raw)
    fit.write_glb(OUTPUT / 'base_rugged.glb', assembled, assembled_raw)
    hair = fit_hair((points, faces, dense))
    (OUTPUT / 'neck-seam-validation.json').write_text(json.dumps(dict(body_seam=body_seam.tolist(), lower=neck['points'][:seam['lower_row_count']].tolist(), upper=neck['points'][seam['upper_row_start']:].tolist(), upper_start=seam['upper_row_start']), indent=2) + '\n')
    report = dict(date='2026-10-04', source_sha256=fit.digest(source_path), base_sha256=fit.digest(io.PARTS / 'fitted/base.glb'),
        source_triangles=4046, fitted_head_triangles=len(faces), collar_triangles=len(neck['faces']), retained_neck_triangles=len(lower_neck['faces']), neck_seam=seam,
        canonical_rig_preserved=True, face_texture_bytes_preserved=True, hair_refitting=hair,
        non_head_body_geometry_uv_weights_and_normals_outside_upper_neck_preserved=body_preserved,
        neck_surface=dict(revision=5, retained_canonical_lower_neck=True, shared_boundary_normals=True,
            retained_neck_band_height_m=.004, original_chin_shelf_removed=True,
            retained_neck_front_outward_growth_removed=True,
            interior_contour_smoothing=True, maximum_outward_overlap_m=0,
            normal_smoothing_sigma_m=.010, body_neck_shading_blend_y_m=[1.59, 1.62],
            texture='512x512 neck skin projection with filtered color transition and preserved donor detail; original face JPEG unchanged',
            deformed_neck_material=dict(original_base_color_preserved=True,obsolete_normal_and_roughness_maps_removed=True,roughness=.9,metallic=0),
            closeup_limitations='Some texture stretching remains at the rear neck'),
        files={name: fit.digest(OUTPUT / name) for name in ['face_rugged.glb', 'base_rugged.glb', 'hair_wavy_bone_rugged.glb', 'neck-transition.png']},
        validations=[fit.validate(OUTPUT / name) for name in ['face_rugged.glb', 'base_rugged.glb', 'hair_wavy_bone_rugged.glb']])
    (ROOT / 'doc/assets/modular-rugged-face-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
