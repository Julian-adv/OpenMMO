import copy
import hashlib
import io
import json
from pathlib import Path

import numpy as np
import trimesh
from PIL import Image
from scipy.ndimage import binary_dilation
from scipy.spatial import cKDTree

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor, add_accessor
from lib.female_custom_eyelashes import PATCHES, LOWER_PATCHES, select_lashes, lash_uv, fit_lash_roots, sweep_outer_upper_lashes, make_texture, repair_corner_maps
from lib.female_custom_eyes import repair_eye_textures, remap_eye_uv

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
SOURCE = OUT / 'source.glb'


def unit(values):
    return values / np.maximum(np.linalg.norm(values, axis=-1, keepdims=True), 1e-12)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def smoothstep(values):
    values = np.clip(values, 0, 1)
    return values * values * (3 - 2 * values)


def smooth_normals(points, faces):
    unique, inverse = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    normals = np.zeros_like(unique)
    triangle = points[faces]
    cross = np.cross(triangle[:, 1] - triangle[:, 0], triangle[:, 2] - triangle[:, 0])
    for corner in range(3):
        np.add.at(normals, inverse[faces[:, corner]], cross)
    return unit(normals)[inverse]


def geometry(doc, raw, primitive, joints=65):
    attrs = {key: accessor(doc, raw, index) for key, index in primitive['attributes'].items()}
    if 'JOINTS_0' in attrs:
        dense = np.zeros((len(attrs['POSITION']), joints))
        np.add.at(dense, (np.arange(len(dense))[:, None], attrs.pop('JOINTS_0')), attrs.pop('WEIGHTS_0'))
        attrs['dense'] = dense
    return attrs, accessor(doc, raw, primitive['indices']).reshape(-1, 3)


def clip(attrs, faces, height, above):
    heights = height(attrs['POSITION']) if callable(height) else height
    score = (attrs['POSITION'][:, 1].astype(np.float64) - heights) * (1 if above else -1)
    score = list(score)
    values = {key: list(value.astype(np.float64)) for key, value in attrs.items()}
    intersections, output = {}, []
    for face in faces:
        polygon = []
        for a, b in zip(face, np.roll(face, -1)):
            if score[a] >= 0:
                polygon.append(a)
            if (score[a] < 0) != (score[b] < 0):
                key = tuple(sorted((int(a), int(b))))
                if key not in intersections:
                    t = score[a] / (score[a] - score[b])
                    intersections[key] = len(score)
                    for name, data in values.items():
                        data.append(data[a] * (1 - t) + data[b] * t)
                    score.append(0)
                polygon.append(intersections[key])
        output.extend([polygon[0], polygon[i], polygon[i + 1]] for i in range(1, len(polygon) - 1))
    used, inverse = np.unique(output, return_inverse=True)
    result = {key: np.asarray(value)[used] for key, value in values.items()}
    result['NORMAL'] = unit(result['NORMAL'])
    return result, inverse.reshape(-1, 3)


def ring(attrs, faces, height):
    points = attrs['POSITION']
    _, first, welded = np.unique(np.round(points, 8), axis=0, return_index=True, return_inverse=True)
    indices = welded[faces]
    edges = np.sort(np.concatenate([indices[:, [0, 1]], indices[:, [1, 2]], indices[:, [2, 0]]]), axis=1)
    edges, counts = np.unique(edges, axis=0, return_counts=True)
    boundary = points[first[edges]]
    heights = height(boundary) if callable(height) else height
    edges = edges[(counts == 1) & np.all(np.abs(boundary[..., 1] - heights) < 1e-5, axis=1)]
    links = {}
    for a, b in edges:
        links.setdefault(a, []).append(b)
        links.setdefault(b, []).append(a)
    assert links and all(len(values) == 2 for values in links.values()), 'Neck cut must form one ring'
    start = min(links)
    ordered, previous, current = [], None, start
    while not ordered or current != start:
        ordered.append(current)
        following = next(i for i in links[current] if i != previous)
        previous, current = current, following
    assert len(ordered) == len(links), 'Multiple neck cut loops'
    ids = first[ordered]
    contour = points[ids]
    area = np.sum(contour[:, 2] * np.roll(contour[:, 0], -1) - contour[:, 0] * np.roll(contour[:, 2], -1))
    if area < 0:
        ids = ids[::-1]
    return np.roll(ids, -np.argmin(points[ids, 2]))


def fractions(points):
    lengths = np.linalg.norm(np.roll(points[:, [0, 2]], -1, axis=0) - points[:, [0, 2]], axis=1)
    return np.r_[0, np.cumsum(lengths)] / lengths.sum()


def sample(values, stops, targets):
    return np.column_stack([np.interp(targets, stops, np.r_[values[:, i], values[0, i]]) for i in range(values.shape[1])])


def surface_sample(points, source, faces, key):
    mesh = trimesh.Trimesh(source['POSITION'], faces, process=False)
    output = []
    for start in range(0, len(points), 2048):
        nearest, _, indices = trimesh.proximity.closest_point(mesh, points[start:start + 2048])
        bary = trimesh.triangles.points_to_barycentric(mesh.triangles[indices], nearest)
        output.append(np.einsum('ni,nij->nj', bary, source[key][faces[indices]]))
    return np.concatenate(output)


def refine_ring(attrs, faces, ids, stops, targets, height):
    points = attrs['POSITION'][ids]
    edge_targets = {}
    for i in range(len(ids)):
        selected = targets[(targets > stops[i] + 1e-10) & (targets < stops[i + 1] - 1e-10)]
        if not len(selected):
            continue
        key = (tuple(np.round(points[i], 8)), tuple(np.round(points[(i + 1) % len(ids)], 8)))
        values = (selected - stops[i]) / (stops[i + 1] - stops[i])
        edge_targets[key] = values
        edge_targets[key[::-1]] = 1 - values[::-1]
    values = {key: list(value) for key, value in attrs.items()}
    output = []
    for triangle in faces:
        polygon = []
        anchor = next((i for i in triangle if abs(attrs['POSITION'][i, 1] - height) > 1e-6), triangle[0])
        triangle = np.roll(triangle, -int(np.flatnonzero(triangle == anchor)[0]))
        for a, b in zip(triangle, np.roll(triangle, -1)):
            polygon.append(int(a))
            key = (tuple(np.round(attrs['POSITION'][a], 8)), tuple(np.round(attrs['POSITION'][b], 8)))
            for t in edge_targets.get(key, []):
                polygon.append(len(values['POSITION']))
                for name, data in values.items():
                    data.append(data[a] * (1 - t) + data[b] * t)
        output.extend([polygon[0], polygon[i], polygon[i + 1]] for i in range(1, len(polygon) - 1))
    return {key: np.asarray(value) for key, value in values.items()}, np.asarray(output)


def direct_seam(body, body_faces, head, head_faces, height):
    lower, upper = ring(body, body_faces, height), ring(head, head_faces, height)
    bottom, top = body['POSITION'][lower].copy(), head['POSITION'][upper].copy()
    a, b = fractions(bottom), fractions(top)
    targets = np.unique(np.r_[a, b])
    targets[-1] = 1
    body, body_faces = refine_ring(body, body_faces, lower, a, targets, height)
    head, head_faces = refine_ring(head, head_faces, upper, b, targets, height)
    canonical = sample(bottom, a, targets[:-1])
    destination = sample(top, b, targets[:-1])
    delta = destination - canonical
    selected = body['POSITION'][:, 1] > height - .05
    points = body['POSITION'][selected]
    start = canonical[:, [0, 2]]
    edge = np.roll(start, -1, axis=0) - start
    relative = points[:, None, [0, 2]] - start[None]
    t = np.clip(np.einsum('nij,ij->ni', relative, edge) / np.maximum(np.sum(edge * edge, axis=1), 1e-15), 0, 1)
    distance = np.sum((relative - t[..., None] * edge) ** 2, axis=2)
    nearest = np.argmin(distance, axis=1)
    blend = t[np.arange(len(points)), nearest, None]
    correction = delta[nearest] * (1 - blend) + delta[(nearest + 1) % len(delta)] * blend
    fade = smoothstep((points[:, 1] - height + .05) / .05)
    original = body['POSITION'].copy()
    body['POSITION'][selected] += correction * fade[:, None]
    seam_positions = np.round(canonical, 8)
    for old, new in zip(seam_positions, destination):
        ids = np.flatnonzero(np.all(np.round(original, 8) == old, axis=1))
        body['POSITION'][ids] = new
    body['NORMAL'][selected] = smooth_normals(body['POSITION'], body_faces)[selected]
    head_positions = np.round(head['POSITION'], 8)
    for point in destination:
        lower_ids = np.flatnonzero(np.all(np.round(body['POSITION'], 8) == np.round(point, 8), axis=1))
        upper_ids = np.flatnonzero(np.all(head_positions == np.round(point, 8), axis=1))
        assert len(lower_ids) and len(upper_ids), 'Direct seam must share every boundary vertex'
        head['dense'][upper_ids] = body['dense'][lower_ids[0]]
        body['NORMAL'][lower_ids] = unit(head['NORMAL'][upper_ids[:1]])
    return body, body_faces, head, head_faces, len(destination), float(np.linalg.norm(body['POSITION'] - original, axis=1).max() * 1000)


def texture_image(doc, raw, material):
    texture = doc['textures'][material['pbrMetallicRoughness']['baseColorTexture']['index']]
    image = doc['images'][texture['source']]
    return np.asarray(Image.open(io.BytesIO(view_bytes(doc, raw, image['bufferView']))).convert('RGB'), dtype=np.float32) / 255


def texture_sample(image, uv):
    height, width = image.shape[:2]
    p = np.clip(uv, 0, 1) * [width - 1, height - 1]
    x, y = np.floor(p).astype(int).T
    dx, dy = (p - np.floor(p)).T
    x1, y1 = np.minimum(x + 1, width - 1), np.minimum(y + 1, height - 1)
    return ((image[y, x] * (1 - dx[:, None]) + image[y, x1] * dx[:, None]) * (1 - dy[:, None]) +
            (image[y1, x] * (1 - dx[:, None]) + image[y1, x1] * dx[:, None]) * dy[:, None])


def bake_body_neck(body, faces, source, source_faces, body_image, head_image, height):
    pixels = body_image.copy()
    image_height, image_width = pixels.shape[:2]
    coordinates, positions = [], []
    for triangle in faces:
        points = body['POSITION'][triangle]
        if points[:, 1].max() < height - .035 or np.abs(points[:, 0]).min() > .075:
            continue
        uv = body['TEXCOORD_0'][triangle] * [image_width - 1, image_height - 1]
        low = np.maximum(np.floor(uv.min(0)).astype(int), 0)
        high = np.minimum(np.ceil(uv.max(0)).astype(int), [image_width - 1, image_height - 1])
        x, y = np.meshgrid(np.arange(low[0], high[0] + 1), np.arange(low[1], high[1] + 1))
        samples = np.column_stack([x.ravel(), y.ravel()])
        matrix = np.column_stack([uv[1] - uv[0], uv[2] - uv[0]])
        if abs(np.linalg.det(matrix)) < 1e-10:
            continue
        weights = (samples - uv[0]) @ np.linalg.inv(matrix).T
        bary = np.column_stack([1 - weights.sum(1), weights])
        inside = np.all(bary >= -1e-6, axis=1)
        world = bary[inside] @ points
        selected = (world[:, 1] >= height - .035) & (np.abs(world[:, 0]) < .075)
        coordinates.append(samples[inside][selected])
        positions.append(world[selected])
    coordinates, positions = np.concatenate(coordinates), np.concatenate(positions)
    uv = surface_sample(positions, source, source_faces, 'TEXCOORD_0')
    color = texture_sample(head_image, uv)
    fade = smoothstep((positions[:, 1] - height + .035) / .035)
    fade *= 1 - smoothstep((np.abs(positions[:, 0]) - .06) / .015)
    x, y = coordinates.T
    pixels[y, x] = pixels[y, x] * (1 - fade[:, None]) + color * fade[:, None]
    painted = np.zeros(pixels.shape[:2], dtype=bool)
    painted[y, x] = True
    padding = np.column_stack(np.nonzero(binary_dilation(painted, iterations=3) & ~painted))
    filled = np.column_stack(np.nonzero(painted))
    nearest = cKDTree(filled).query(padding)[1]
    pixels[padding[:, 0], padding[:, 1]] = pixels[filled[nearest, 0], filled[nearest, 1]]
    Image.fromarray(np.round(np.clip(pixels, 0, 1) * 255).astype(np.uint8)).save(OUT / 'body-neck-albedo.png')
    return int(len(np.unique(coordinates, axis=0)))


def add_material(doc, raw, source, source_raw):
    image_offset, texture_offset = len(doc['images']), len(doc['textures'])
    sampler_offset = len(doc.setdefault('samplers', []))
    for image in source['images']:
        data = view_bytes(source, source_raw, image['bufferView'])
        raw.extend(b'\0' * (-len(raw) % 4))
        index = len(doc['bufferViews'])
        doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(data)})
        raw.extend(data)
        doc['images'].append({**image, 'bufferView': index})
    doc['samplers'].extend(copy.deepcopy(source.get('samplers', [])))
    for texture in source['textures']:
        texture = copy.deepcopy(texture)
        texture['source'] += image_offset
        if 'sampler' in texture:
            texture['sampler'] += sampler_offset
        doc['textures'].append(texture)
    material = copy.deepcopy(source['materials'][0])
    material['name'] = 'Female custom face original textures'
    def remap(value):
        if isinstance(value, dict):
            for key, child in value.items():
                if key.endswith('Texture') and isinstance(child, dict) and 'index' in child:
                    child['index'] += texture_offset
                else:
                    remap(child)
    remap(material)
    doc['materials'].append(material)
    return len(doc['materials']) - 1


def add_geometry(doc, raw, attrs, faces, material):
    attrs = {key: value.copy() for key, value in attrs.items()}
    attrs['NORMAL'] = unit(attrs['NORMAL'])
    dense = attrs.pop('dense')
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    weights /= weights.sum(1, keepdims=True)
    attrs['JOINTS_0'], attrs['WEIGHTS_0'] = joints, weights
    kinds = {'POSITION': 'VEC3', 'NORMAL': 'VEC3', 'TEXCOORD_0': 'VEC2', 'TEXCOORD_1': 'VEC2', 'JOINTS_0': 'VEC4', 'WEIGHTS_0': 'VEC4'}
    indices = {key: add_accessor(doc, raw, value, kinds[key], 5123 if key == 'JOINTS_0' else 5126) for key, value in attrs.items()}
    return {'attributes': indices, 'indices': add_accessor(doc, raw, faces.reshape(-1, 1), 'SCALAR', 5125), 'material': material}


def compact(doc, raw):
    used_materials = sorted({p['material'] for m in doc['meshes'] for p in m['primitives']})
    materials = {old: new for new, old in enumerate(used_materials)}
    doc['materials'] = [doc['materials'][i] for i in used_materials]
    texture_refs = []
    def collect(value):
        if isinstance(value, dict):
            for key, child in value.items():
                if key.endswith('Texture') and isinstance(child, dict) and 'index' in child:
                    texture_refs.append(child)
                else:
                    collect(child)
    for material in doc['materials']:
        collect(material)
    texture_ids = sorted({ref['index'] for ref in texture_refs})
    textures = {old: new for new, old in enumerate(texture_ids)}
    doc['textures'] = [doc['textures'][i] for i in texture_ids]
    for ref in texture_refs:
        ref['index'] = textures[ref['index']]
    image_ids = sorted({texture['source'] for texture in doc['textures']})
    images = {old: new for new, old in enumerate(image_ids)}
    doc['images'] = [doc['images'][i] for i in image_ids]
    for texture in doc['textures']:
        texture['source'] = images[texture['source']]
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            primitive['material'] = materials[primitive['material']]
    used = {skin['inverseBindMatrices'] for skin in doc.get('skins', [])}
    for animation in doc.get('animations', []):
        for sampler in animation['samplers']:
            used.update([sampler['input'], sampler['output']])
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            used.update(primitive['attributes'].values())
            used.add(primitive['indices'])
    mapping = {old: new for new, old in enumerate(sorted(used))}
    doc['accessors'] = [copy.deepcopy(doc['accessors'][i]) for i in sorted(used)]
    for skin in doc.get('skins', []):
        skin['inverseBindMatrices'] = mapping[skin['inverseBindMatrices']]
    for animation in doc.get('animations', []):
        for sampler in animation['samplers']:
            sampler['input'], sampler['output'] = mapping[sampler['input']], mapping[sampler['output']]
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            primitive['attributes'] = {key: mapping[index] for key, index in primitive['attributes'].items()}
            primitive['indices'] = mapping[primitive['indices']]
    refs = doc['accessors'] + doc['images']
    views = sorted({ref['bufferView'] for ref in refs})
    mapping = {old: new for new, old in enumerate(views)}
    packed, output = bytearray(), []
    for index in views:
        data = view_bytes(doc, raw, index)
        packed.extend(b'\0' * (-len(packed) % 4))
        view = copy.deepcopy(doc['bufferViews'][index])
        view.update(byteOffset=len(packed), byteLength=len(data))
        output.append(view)
        packed.extend(data)
    for ref in refs:
        ref['bufferView'] = mapping[ref['bufferView']]
    doc['bufferViews'], doc['buffers'] = output, [{'byteLength': len(packed)}]
    return bytes(packed)


def main():
    assert digest(BASE) == '3eb7c0a5f3c6b30aec0fbae0b70c8fd925e4cb91772647420007eee795a1e0a5'
    assert digest(SOURCE) == '8dbcbcb21d06cd2ed33020a9eef617e1b4396fa4e43292c95748ddbbe3152dce'
    base, base_raw = read_glb(BASE)
    source, source_raw = read_glb(SOURCE)
    original_body, body_faces = geometry(base, base_raw, base['meshes'][0]['primitives'][0])
    source_attrs, source_faces = geometry(source, source_raw, source['meshes'][0]['primitives'][0])
    source_attrs['lash_mask'] = select_lashes(source_attrs['POSITION'], source_faces)
    source_attrs['source_position'] = source_attrs['POSITION'].astype(np.float64)
    scale = 1.8 / float(source_attrs['POSITION'][:, 1].max())
    offset = np.array([-.0008, 0, .014])
    source_attrs['POSITION'] = source_attrs['source_position'] * scale + offset
    height = 1.552
    head, head_faces = clip(source_attrs, source_faces, height, True)
    head['dense'] = surface_sample(head['POSITION'], original_body, body_faces, 'dense')
    head_joint = [base['nodes'][i]['name'] for i in base['skins'][0]['joints']].index('Head')
    blend = smoothstep((head['POSITION'][:, 1] - 1.59) / .02)
    head['dense'] *= 1 - blend[:, None]
    head['dense'][:, head_joint] += blend
    body, kept_faces = clip(original_body, body_faces, height, False)
    body, kept_faces, head, head_faces, seam_count, body_displacement = direct_seam(body, kept_faces, head, head_faces, height)
    reference = head.pop('source_position') * scale + offset
    lash_kinds = head.pop('lash_mask')[:, 0]
    lashes = np.all(lash_kinds[head_faces] > .5, axis=1)
    assert int(lashes.sum()) == int(np.all(source_attrs['lash_mask'][source_faces, 0] > .5, axis=1).sum())
    affine_error = float(np.linalg.norm(head['POSITION'] - reference, axis=1).max())
    assert affine_error < 1e-12, 'The new head must use only uniform scale and translation'
    body_material = base['meshes'][0]['primitives'][0]['material']
    recolored = bake_body_neck(body, kept_faces, source_attrs, source_faces,
                              texture_image(base, base_raw, base['materials'][body_material]),
                              texture_image(source, source_raw, source['materials'][0]), height)
    doc, raw = copy.deepcopy(base), bytearray(base_raw)
    face_material = add_material(doc, raw, source, source_raw)
    original_material = source['materials'][0]
    corner_images = {}
    for role, texture in [('normal', original_material['normalTexture']['index']),
                          ('orm', original_material['pbrMetallicRoughness']['metallicRoughnessTexture']['index'])]:
        image = source['images'][source['textures'][texture]['source']]
        corner_images[role] = Image.open(io.BytesIO(view_bytes(source, source_raw, image['bufferView'])))
    corner_record = repair_corner_maps(source_attrs['POSITION'], source_faces, source_attrs['TEXCOORD_0'], corner_images, OUT)
    color_image = source['images'][source['textures'][original_material['pbrMetallicRoughness']['baseColorTexture']['index']]['source']]
    eye_images = {'albedo': Image.open(io.BytesIO(view_bytes(source, source_raw, color_image['bufferView']))),
                  'normal': Image.open(OUT / 'face-corner-normal.png'), 'orm': Image.open(OUT / 'face-corner-orm.png')}
    eye_record = repair_eye_textures(source_attrs['POSITION'], source_faces, source_attrs['TEXCOORD_0'], eye_images, OUT)
    doc['materials'][face_material]['name'] = 'Female custom face cleaned eyes'
    image_data = (OUT / 'body-neck-albedo.png').read_bytes()
    raw.extend(b'\0' * (-len(raw) % 4))
    view = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(image_data)})
    raw.extend(image_data)
    doc['images'].append({'bufferView': view, 'mimeType': 'image/png', 'name': 'Body neck color match'})
    doc['textures'].append({'source': len(doc['images']) - 1})
    doc['materials'][body_material]['pbrMetallicRoughness']['baseColorTexture']['index'] = len(doc['textures']) - 1
    lash_record = make_texture(OUT / 'eyelash-alpha.png')
    fit_texture = OUT / 'eyelash-fit-alpha.png'
    make_texture(fit_texture, width_scale=1, root_width_scale=1)
    image_data = (OUT / 'eyelash-alpha.png').read_bytes()
    raw.extend(b'\0' * (-len(raw) % 4))
    view = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(image_data)})
    raw.extend(image_data)
    doc['images'].append({'bufferView': view, 'mimeType': 'image/png', 'name': 'Custom female eyelash alpha'})
    doc['samplers'].append({'wrapS': 33071, 'wrapT': 33071, 'magFilter': 9729, 'minFilter': 9987})
    doc['textures'].append({'source': len(doc['images']) - 1, 'sampler': len(doc['samplers']) - 1})
    doc['materials'].append({'name': 'Custom female eyelashes', 'alphaMode': 'MASK', 'alphaCutoff': .18, 'doubleSided': True,
                            'pbrMetallicRoughness': {'baseColorTexture': {'index': len(doc['textures']) - 1, 'texCoord': 1},
                                                     'metallicFactor': 0, 'roughnessFactor': .9}})
    opaque_ids, opaque_faces = np.unique(head_faces[~lashes], return_inverse=True)
    lash_ids, lash_faces = np.unique(head_faces[lashes], return_inverse=True)
    opaque = {key: value[opaque_ids] for key, value in head.items()}
    lash_attrs = {key: value[lash_ids] for key, value in head.items()}
    lash_attrs['TEXCOORD_1'] = lash_uv(lash_attrs['POSITION'], lash_kinds[lash_ids])
    lash_attrs['POSITION'], attachment = fit_lash_roots(lash_attrs['POSITION'], lash_faces.reshape(-1, 3),
                                                        lash_attrs['TEXCOORD_1'], lash_kinds[lash_ids],
                                                        opaque['POSITION'], opaque_faces.reshape(-1, 3), fit_texture, 1)
    upper_lashes = lash_kinds[lash_ids] == 1
    lash_normals = smooth_normals(lash_attrs['POSITION'], lash_faces.reshape(-1, 3))
    lash_attrs['NORMAL'][upper_lashes] = lash_normals[upper_lashes]
    lash_attrs['POSITION'], lower_attachment = fit_lash_roots(lash_attrs['POSITION'], lash_faces.reshape(-1, 3),
                                                            lash_attrs['TEXCOORD_1'], lash_kinds[lash_ids],
                                                            opaque['POSITION'], opaque_faces.reshape(-1, 3), fit_texture, 2)
    lash_normals = smooth_normals(lash_attrs['POSITION'], lash_faces.reshape(-1, 3))
    lash_attrs['NORMAL'][~upper_lashes] = lash_normals[~upper_lashes]
    lash_attrs['POSITION'], outer_sweep = sweep_outer_upper_lashes(lash_attrs['POSITION'], lash_attrs['TEXCOORD_1'],
                                                                 lash_kinds[lash_ids], attachment)
    lash_normals = smooth_normals(lash_attrs['POSITION'], lash_faces.reshape(-1, 3))
    lash_attrs['NORMAL'][upper_lashes] = lash_normals[upper_lashes]
    opaque, opaque_faces, eye_parents, tiles = remap_eye_uv(opaque, opaque_faces.reshape(-1, 3), eye_images, OUT)
    opaque_ids = opaque_ids[eye_parents]
    eye_record['atlas_tiles'] = tiles
    eye_record['eye_uv_remapped'] = True
    eye_record['geometry_rig_and_lashes_preserved'] = True
    eye_record.pop('geometry_uv_rig_and_lashes_preserved')
    eye_mask = np.asarray(Image.open(OUT / 'eye-cleanup-mask.png')) > 0
    eye_record['mask_pixels'] = int(eye_mask.sum())
    eye_record['duplicated_vertices'] = sum(tile['vertices'] for tile in tiles)
    for role in ['albedo', 'normal', 'orm']:
        pixels = np.asarray(Image.open(OUT / f'face-eye-{role}.png'))
        original_pixels = np.asarray(eye_images[role].convert('RGB'))
        assert np.array_equal(pixels[~eye_mask], original_pixels[~eye_mask])
        eye_record[role + '_changed_pixels'] = int(np.any(pixels != original_pixels, axis=2).sum())
        data = (OUT / f'face-eye-{role}.png').read_bytes()
        raw.extend(b'\0' * (-len(raw) % 4))
        view = len(doc['bufferViews'])
        doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(data)})
        raw.extend(data)
        doc['images'].append({'bufferView': view, 'mimeType': 'image/png', 'name': f'Face cleaned eyes {role}'})
        doc['textures'].append({'source': len(doc['images']) - 1})
        if role == 'normal':
            doc['materials'][face_material]['normalTexture']['index'] = len(doc['textures']) - 1
        elif role == 'orm':
            doc['materials'][face_material]['pbrMetallicRoughness']['metallicRoughnessTexture']['index'] = len(doc['textures']) - 1
        else:
            doc['materials'][face_material]['pbrMetallicRoughness']['baseColorTexture']['index'] = len(doc['textures']) - 1
    primitives = [add_geometry(doc, raw, attrs, faces, material) for attrs, faces, material in
                  [(body, kept_faces, body_material), (opaque, opaque_faces.reshape(-1, 3), face_material),
                   (lash_attrs, lash_faces.reshape(-1, 3), len(doc['materials']) - 1)]]
    doc['meshes'] = [{'name': 'female_body', 'primitives': [primitives[0]]},
                     {'name': 'face_female_custom', 'primitives': [primitives[1]]},
                     {'name': 'eyelashes_female_custom', 'primitives': [primitives[2]]}]
    doc['nodes'][0]['mesh'] = 0
    doc['nodes'].append({'name': 'face_female_custom', 'mesh': 1, 'skin': 0,
                         'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'face_female_custom'}})
    doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
    doc['nodes'].append({'name': 'eyelashes_female_custom', 'mesh': 2, 'skin': 0,
                         'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'eyelashes_female_custom'}})
    doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
    assembled_path = OUT / 'base_custom.glb'
    write_glb(assembled_path, doc, compact(doc, raw))
    face_doc, face_raw = read_glb(assembled_path)
    face_doc.pop('animations', None)
    face_doc['nodes'][0].pop('mesh')
    face_doc['nodes'][0].pop('skin')
    face_doc['meshes'] = face_doc['meshes'][1:]
    for node in face_doc['nodes']:
        if 'mesh' in node:
            node['mesh'] -= 1
    write_glb(OUT / 'face_custom.glb', face_doc, compact(face_doc, face_raw))
    assembled, assembled_raw = read_glb(assembled_path)
    exported_face_points = accessor(assembled, assembled_raw, assembled['meshes'][1]['primitives'][0]['attributes']['POSITION'])
    exported_affine_error = float(np.linalg.norm(exported_face_points - reference[opaque_ids], axis=1).max())
    assert exported_affine_error < 1.2e-7
    lower, upper = ring(body, kept_faces, height), ring(head, head_faces, height)
    assert {tuple(np.round(point, 8)) for point in body['POSITION'][lower]} == {tuple(np.round(point, 8)) for point in head['POSITION'][upper]}
    assert assembled['nodes'][:len(base['nodes'])] == base['nodes']
    assert np.array_equal(accessor(base, base_raw, base['skins'][0]['inverseBindMatrices']),
                          accessor(assembled, assembled_raw, assembled['skins'][0]['inverseBindMatrices']))
    for original, fitted in zip(base['animations'], assembled['animations']):
        assert original['channels'] == fitted['channels']
        for before_sampler, after_sampler in zip(original['samplers'], fitted['samplers']):
            for key in ['input', 'output']:
                assert np.array_equal(accessor(base, base_raw, before_sampler[key]),
                                      accessor(assembled, assembled_raw, after_sampler[key]))
    report = {'date': '2026-11-07', 'source_path': str(SOURCE.relative_to(ROOT)), 'source_sha256': digest(SOURCE),
              'base_sha256': digest(BASE), 'scale': float(scale), 'offset_m': offset.tolist(),
              'neck_cut_y_m': height, 'neck_seams': 1, 'neck_bridge_triangles': 0, 'rig_id': 'human_female_01_local_v1',
              'new_head_scale_and_translation_only': True, 'maximum_head_affine_error_m': affine_error,
              'head_affine_scope': 'Face, scalp and neck; fitted upper and lower eyelashes excluded',
              'maximum_exported_head_affine_error_m': exported_affine_error,
              'source_triangles': len(source_faces), 'head_triangles': len(head_faces),
              'retained_body_triangles': len(kept_faces), 'assembled_triangles': len(kept_faces) + len(head_faces),
              'eyelashes': {'triangles': int(lashes.sum()), 'upper_triangles': int(np.all(lash_kinds[head_faces] == 1, axis=1).sum()),
                            'lower_triangles': int(np.all(lash_kinds[head_faces] == 2, axis=1).sum()),
                            'source_uv_components': PATCHES + LOWER_PATCHES,
                            'geometry_preserved': False, 'topology_preserved': True, 'original_uv0_preserved': True, 'new_uv_channel': 1,
                            'upper_root_attachment': attachment,
                            'lower_root_attachment': lower_attachment,
                            'outer_upper_sweep': outer_sweep,
                            'method': 'Fitted upper and lower lash roots; dedicated nonmetallic MASK material; skin outside eye cleanup preserved', **lash_record},
              'eye_corner_material_repair': corner_record,
              'eye_texture_repair': eye_record,
              'neck_boundary_vertices': seam_count, 'source_texture_bytes_preserved': False, 'source_color_texture_bytes_preserved': False,
              'source_color_pixels_outside_eye_mask_preserved': True,
              'canonical_bone_nodes_bind_matrices_and_animation_tracks_preserved': True,
              'body_neck_fit_maximum_mm': body_displacement, 'body_neck_recolored_pixels': recolored,
              'files': {name: digest(OUT / name) for name in ['base_custom.glb', 'face_custom.glb', 'body-neck-albedo.png', 'eyelash-alpha.png', 'eyelash-fit-alpha.png', 'face-corner-normal.png', 'face-corner-orm.png', 'face-eye-albedo.png', 'face-eye-normal.png', 'face-eye-orm.png', 'eye-cleanup-mask.png', 'eye-iris-clean-v1.png']},
              'status': 'Direct seam geometry validated; run animation and visual review after fitting'}
    (OUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
    (ROOT / 'doc/assets/modular-female-custom-face-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
