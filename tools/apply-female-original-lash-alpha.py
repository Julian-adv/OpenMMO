import copy
import hashlib
import importlib.util
import io
import json
import shutil
from pathlib import Path

import numpy as np
from PIL import Image

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor, add_accessor
from lib.female_custom_eyelashes import select_lashes, uv_labels


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
WORK = OUT / 'lash_skin_alpha_v1'
BASELINE = WORK / 'before'
SPEC = importlib.util.spec_from_file_location('female_fitter', ROOT / 'tools/fit-female-custom-face.py')
FIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIT)


def triangle_key(points, uv):
    values = np.column_stack([points, uv]).astype('<f4')
    return b''.join(sorted(vertex.tobytes() for vertex in values))


def sample_color(image, uv):
    height, width = image.shape[:2]
    points = uv * [width, height] - .5
    x, y = np.floor(points).astype(int).T
    dx, dy = (points - np.floor(points)).T
    return ((image[y % height, x % width, :3] * (1 - dx[:, None]) + image[y % height, (x + 1) % width, :3] * dx[:, None]) * (1 - dy[:, None])
            + (image[(y + 1) % height, x % width, :3] * (1 - dx[:, None]) + image[(y + 1) % height, (x + 1) % width, :3] * dx[:, None]) * dy[:, None])


def embed_texture(doc, raw, image):
    stream = io.BytesIO()
    image.save(stream, format='PNG')
    raw.extend(b'\0' * (-len(raw) % 4))
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(stream.getvalue())})
    raw.extend(stream.getvalue())
    doc['images'].append({'bufferView': len(doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': 'Original eyelash color with skin alpha'})
    doc.setdefault('samplers', []).append({'wrapS': 33071, 'wrapT': 33071, 'magFilter': 9729, 'minFilter': 9987})
    doc['textures'].append({'source': len(doc['images']) - 1, 'sampler': len(doc['samplers']) - 1})
    return len(doc['textures']) - 1


def add_primitive(doc, raw, attrs, faces, material):
    kinds = {'POSITION': 'VEC3', 'NORMAL': 'VEC3', 'TEXCOORD_0': 'VEC2', 'TEXCOORD_1': 'VEC2', 'JOINTS_0': 'VEC4', 'WEIGHTS_0': 'VEC4'}
    indices = {key: add_accessor(doc, raw, value, kinds[key], 5123 if key == 'JOINTS_0' else 5126) for key, value in attrs.items()}
    return {'attributes': indices, 'indices': add_accessor(doc, raw, faces.reshape(-1, 1), 'SCALAR', 5125), 'material': material}


def main():
    BASELINE.mkdir(parents=True, exist_ok=True)
    if not (BASELINE / 'base_custom.glb').exists():
        assert json.loads((OUT / 'fitting.json').read_text())['neck_fit_only']
        for name in ['base_custom.glb', 'base_custom_preview.glb', 'face_custom.glb', 'custom-face-fitting.blend', 'fitting.json']:
            shutil.copyfile(OUT / name, BASELINE / name)
    fitting = json.loads((BASELINE / 'fitting.json').read_text())
    doc, baseline_raw = read_glb(BASELINE / 'base_custom.glb')
    baseline_doc = copy.deepcopy(doc)
    raw = bytearray(baseline_raw)
    primitive = doc['meshes'][1]['primitives'][0]
    attrs = {key: accessor(doc, baseline_raw, value) for key, value in primitive['attributes'].items()}
    faces = accessor(doc, baseline_raw, primitive['indices']).reshape(-1, 3)
    source, source_raw = read_glb(OUT / 'source.glb')
    item = source['meshes'][0]['primitives'][0]
    points = accessor(source, source_raw, item['attributes']['POSITION'])
    source_uv = accessor(source, source_raw, item['attributes']['TEXCOORD_0'])
    source_faces = accessor(source, source_raw, item['indices']).reshape(-1, 3)
    kinds = select_lashes(points, source_faces)[:, 0]
    labels = uv_labels(points, source_faces)
    selected = np.all(kinds[source_faces] > .5, axis=1)
    world = (points.astype(float) * fitting['scale'] + fitting['offset_m']).astype(np.float32)
    lookup = {triangle_key(world[source_faces[i]], source_uv[source_faces[i]]):
              (int(labels[source_faces[i, 0]]), int(kinds[source_faces[i, 0]]), int(i)) for i in np.flatnonzero(selected)}
    matched = [lookup.get(triangle_key(attrs['POSITION'][f], attrs['TEXCOORD_0'][f])) for f in faces]
    candidates = np.array([item is not None for item in matched])
    assert int(candidates.sum()) == int(selected.sum()) == 508
    protection_file = WORK / 'eye-surface-protection.json'
    protected = set(json.loads(protection_file.read_text())['source_faces']) if protection_file.exists() else set()
    assert protected.issubset(set(np.flatnonzero(selected)))
    alpha_faces = np.array([item is not None and item[2] not in protected for item in matched])
    face_material = primitive['material']
    original_material = copy.deepcopy(doc['materials'][face_material])
    texture = doc['textures'][original_material['pbrMetallicRoughness']['baseColorTexture']['index']]
    image = doc['images'][texture['source']]
    rgb = np.asarray(Image.open(io.BytesIO(view_bytes(doc, baseline_raw, image['bufferView']))).convert('RGB'))
    height, width = rgb.shape[:2]
    groups = np.array([item[0] if item else -1 for item in matched])
    crops = []
    for label in sorted(set(groups[candidates])):
        ids = np.unique(faces[groups == label])
        pixels = attrs['TEXCOORD_0'][ids] * [width, height] - .5
        low = np.floor(pixels.min(0)).astype(int) - 3
        high = np.ceil(pixels.max(0)).astype(int) + 4
        crops.append({'label': int(label), 'low': low, 'size': high - low})
    size = 256
    while True:
        x = y = row = 0
        for crop in sorted(crops, key=lambda item: -item['size'][1]):
            w, h = crop['size']
            if x + w > size:
                x, y, row = 0, y + row, 0
            crop['origin'] = np.array([x, y])
            x += int(w)
            row = max(row, int(h))
        if y + row <= size:
            break
        size *= 2
    assert size <= 512
    rgba = np.zeros((size, size, 4), dtype=np.uint8)
    for crop in crops:
        x, y = crop['low']; w, h = crop['size']; dx, dy = crop['origin']
        colors = rgb[np.arange(y, y + h)[:, None] % height, np.arange(x, x + w) % width]
        luminance = colors @ np.array([.2126, .7152, .0722])
        skin = np.clip((luminance - 70) / 45, 0, 1)
        skin = skin * skin * (3 - 2 * skin)
        warm = colors.astype(float)
        red = np.clip((warm[..., 0] - warm[..., 1] - 8) / 12, 0, 1)
        yellow = np.clip((warm[..., 1] - warm[..., 2] - 3) / 10, 0, 1)
        light = np.clip((luminance - 30) / 45, 0, 1)
        warm_skin = (red * red * (3 - 2 * red)) * (yellow * yellow * (3 - 2 * yellow)) * (light * light * (3 - 2 * light))
        skin = np.maximum(skin, warm_skin)
        rgba[dy:dy + h, dx:dx + w, :3] = colors
        rgba[dy:dy + h, dx:dx + w, 3] = np.round((1 - skin) * 255).astype(np.uint8)
    atlas = Image.fromarray(rgba)
    atlas.save(WORK / 'original-lash-skin-alpha.png')
    material = copy.deepcopy(original_material)
    material.update(name='Original eyelashes skin alpha only', alphaMode='MASK', alphaCutoff=.5, doubleSided=True)
    material['pbrMetallicRoughness']['baseColorTexture'] = {'index': embed_texture(doc, raw, atlas), 'texCoord': 1}
    doc['materials'].append(material)
    lash_material = len(doc['materials']) - 1
    opaque_ids, opaque_faces = np.unique(faces[~alpha_faces], return_inverse=True)
    opaque = {key: value[opaque_ids] for key, value in attrs.items()}
    lash_values = {key: [] for key in attrs}
    lash_values['TEXCOORD_1'] = []
    lash_faces, remap = [], {}
    crop_map = {item['label']: item for item in crops}
    for face, label in zip(faces[alpha_faces], groups[alpha_faces]):
        triangle = []
        for old in face:
            key = (int(old), int(label))
            if key not in remap:
                remap[key] = len(remap)
                for name in attrs:
                    lash_values[name].append(attrs[name][old])
                crop = crop_map[label]
                lash_values['TEXCOORD_1'].append((attrs['TEXCOORD_0'][old] * [width, height] - crop['low'] + crop['origin']) / size)
            triangle.append(remap[key])
        lash_faces.append(triangle)
    lash = {key: np.asarray(value, dtype=attrs[key].dtype if key in attrs else np.float32) for key, value in lash_values.items()}
    lash_faces = np.asarray(lash_faces)
    for key in attrs:
        np.testing.assert_array_equal(opaque[key][opaque_faces.reshape(-1, 3)], attrs[key][faces[~alpha_faces]])
        np.testing.assert_array_equal(lash[key][lash_faces], attrs[key][faces[alpha_faces]])
    bary = np.array([[1, 0, 0], [0, 1, 0], [0, 0, 1], [1/3, 1/3, 1/3], [.2, .3, .5]])
    old_uv = np.einsum('ki,nij->nkj', bary, lash['TEXCOORD_0'][lash_faces]).reshape(-1, 2)
    new_uv = np.einsum('ki,nij->nkj', bary, lash['TEXCOORD_1'][lash_faces]).reshape(-1, 2)
    color_error = float(np.abs(sample_color(rgb, old_uv) - sample_color(rgba, new_uv)).max())
    assert color_error < .01
    opaque_primitive = add_primitive(doc, raw, opaque, opaque_faces.reshape(-1, 3), face_material)
    lash_primitive = add_primitive(doc, raw, lash, lash_faces, lash_material)
    doc['meshes'][1]['primitives'] = [opaque_primitive]
    doc['meshes'].append({'name': 'eyelashes_female_custom', 'primitives': [lash_primitive]})
    doc['nodes'].append({'name': 'eyelashes_female_custom', 'mesh': 2, 'skin': 0})
    doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
    assert doc['materials'][face_material] == original_material
    assert doc['nodes'][:-1] == baseline_doc['nodes']
    packed = FIT.compact(doc, raw)
    write_glb(OUT / 'base_custom.glb', doc, packed)
    face_doc = copy.deepcopy(doc)
    face_doc.pop('animations', None)
    face_doc['nodes'][0].pop('mesh'); face_doc['nodes'][0].pop('skin')
    face_doc['meshes'] = face_doc['meshes'][1:]
    for node in face_doc['nodes']:
        if 'mesh' in node:
            node['mesh'] -= 1
    write_glb(OUT / 'face_custom.glb', face_doc, FIT.compact(face_doc, packed))
    fitting.update(neck_fit_only=False, original_head_alpha_only=True, eye_and_eyelash_edits_applied=True,
                   geometry_rig_and_animations_unchanged=True, original_lash_rgb_preserved=True, meshes=3, materials=3)
    fitting['eyelashes'] = {'triangles': int(alpha_faces.sum()), 'alpha_mode': 'MASK', 'alpha_cutoff': .5, 'atlas_size': [size, size],
                            'method': 'Original color pixels retained; bright skin tones keyed to transparent alpha',
                            'opaque_luminance_max': 70, 'transparent_luminance_min': 115,
                            'warm_skin_shadow_key': {'red_minus_green': [8, 20], 'green_minus_blue': [3, 13], 'luminance': [30, 75]},
                            'original_uv0_preserved': True, 'alpha_color_uv_channel': 1,
                            'positions_normals_weights_and_triangles_preserved': True}
    fitting['protected_eye_source_faces'] = sorted(protected)
    fitting['status'] = 'Original neck-fitted head; only skin alpha on original eyelash cards is applied'
    fitting['files'].update({name: FIT.digest(OUT / name) for name in ['base_custom.glb', 'face_custom.glb']})
    fitting['files']['lash_skin_alpha_v1/original-lash-skin-alpha.png'] = FIT.digest(WORK / 'original-lash-skin-alpha.png')
    for path in [OUT / 'fitting.json', ROOT / 'doc/assets/modular-female-custom-face-fitting-v1.json']:
        path.write_text(json.dumps(fitting, indent=2) + '\n')
    record = {'date': '2026-11-07', 'source_model_sha256': FIT.digest(OUT / 'source.glb'),
              'baseline_sha256': FIT.digest(BASELINE / 'base_custom.glb'), 'model_sha256': FIT.digest(OUT / 'base_custom.glb'),
              'geometry_unchanged': True, 'original_color_pixels_preserved': True, 'atlas_size': [size, size],
              'sampled_original_rgb_maximum_error_8bit': color_error,
              'selected_source_faces': [int(i) for i in np.flatnonzero(selected) if i not in protected],
              'protected_eye_source_faces': sorted(protected),
              'skin_alpha': fitting['eyelashes'],
              'license_and_source': 'User-provided Tripo model and its original color texture; same unprovided license as source.glb. Alpha channel and packing produced locally; no AI generation.',
              'crops': [{key: value.tolist() if isinstance(value, np.ndarray) else value for key, value in crop.items()} for crop in crops]}
    (WORK / 'build.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps({key: value for key, value in record.items() if key not in ['selected_source_faces', 'crops']}))


if __name__ == '__main__':
    main()
