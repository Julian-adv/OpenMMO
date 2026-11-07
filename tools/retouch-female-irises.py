import argparse
import hashlib
import importlib.util
import io
import json
import shutil
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.ndimage import map_coordinates

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor, add_accessor


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
WORK = OUT / 'iris_texture_cleanup_v2'
BEFORE = WORK / 'before'
SPEC = importlib.util.spec_from_file_location('ridge_fitter', ROOT / 'tools/fit-female-eyelid-ridge.py')
RIDGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RIDGE)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def color_image(doc, raw):
    part = next(mesh for mesh in doc['meshes'] if mesh['name'] == 'face_female_custom')['primitives'][0]
    texture = doc['materials'][part['material']]['pbrMetallicRoughness']['baseColorTexture']
    image = doc['textures'][texture['index']]['source']
    rgb = np.asarray(Image.open(io.BytesIO(view_bytes(doc, raw, doc['images'][image]['bufferView']))).convert('RGB'))
    return part, texture, image, rgb


def prepare(points, faces, uv, rgb):
    height, width = rgb.shape[:2]
    original = rgb.astype(float)
    canvas = np.zeros((512, 1024, 3), dtype=np.uint8)
    for side, offset in [(-1, 0), (1, 512)]:
        xy = (points[:, :2] - [side * .033 - .0008, 1.6846]) / .026 * 512 + 256
        xy[:, 1] = 512 - xy[:, 1]
        depth = np.full((512, 512), -np.inf)
        for face in faces:
            if points[face, 2].max() < .052:
                continue
            sample = RIDGE.raster(xy[face], 512, 512)
            if sample is None:
                continue
            x, y, bary = sample
            z = bary @ points[face, 2]
            take = z > depth[y, x]
            x, y, z, bary = x[take], y[take], z[take], bary[take]
            pixels = bary @ uv[face] * [width, height] - .5
            colors = np.column_stack([map_coordinates(original[..., c], pixels[:, ::-1].T, order=1, mode='wrap') for c in range(3)])
            canvas[y, x + offset] = np.round(colors).astype(np.uint8)
            depth[y, x] = z
    Image.fromarray(canvas).save(WORK / 'original-eyes-flat.png')


def iris_mask(points):
    x, y, z = points.T
    absolute = np.abs(x + .0008)
    radius = np.hypot((absolute - .033) / .0078, (y - 1.6850) / .0058)
    weight = 1 - RIDGE.smooth((radius - .88) / .12)
    top = np.interp(absolute, [.016, .020, .026, .033, .040, .047, .052],
                    [1.6819, 1.6860, 1.6891, 1.6894, 1.6878, 1.6854, 1.6835])
    weight *= 1 - RIDGE.smooth((y - top + .00005) / .00035)
    weight *= RIDGE.smooth((z - .052) / .006)
    return weight


def map_iris_faces(doc, binary, raw, part, tiles, width, height):
    attrs = {key: accessor(doc, binary, index) for key, index in part['attributes'].items()}
    faces = accessor(doc, binary, part['indices']).reshape(-1, 3)
    points = attrs['POSITION']
    selected = (iris_mask(points)[faces].max(1) > 0) | (iris_mask(points[faces].mean(1)) > 0)
    parents = list(range(len(points)))
    color_uv = list(attrs['TEXCOORD_1'])
    remap = {}
    new_faces = faces.copy()
    for i in np.flatnonzero(selected):
        side = -1 if points[faces[i], 0].mean() < -.0008 else 1
        tile = next(tile for tile in tiles if tile['side'] == side)
        for corner, old in enumerate(faces[i]):
            position = points[old]
            planar = [(position[0] - (side * .033 - .0008)) / .060 + .5, (position[1] - 1.681) / -.040 + .5]
            target = (np.asarray(tile['origin']) + np.asarray(planar) * tile['size']) / [width, height]
            if np.max(np.abs(target - attrs['TEXCOORD_1'][old])) < 1e-7:
                continue
            key = (int(old), side)
            if key not in remap:
                remap[key] = len(parents)
                parents.append(int(old))
                color_uv.append(target)
            new_faces[i, corner] = remap[key]
    parents = np.asarray(parents)
    result = {key: value[parents] for key, value in attrs.items()}
    result['TEXCOORD_1'] = np.asarray(color_uv, dtype=np.float32)
    result['TEXCOORD_2'] = attrs['TEXCOORD_1'][parents]
    for key in ['POSITION', 'NORMAL', 'TEXCOORD_0', 'JOINTS_0', 'WEIGHTS_0']:
        np.testing.assert_array_equal(result[key][new_faces], attrs[key][faces])
    np.testing.assert_array_equal(result['TEXCOORD_2'][new_faces], attrs['TEXCOORD_1'][faces])
    types = {'POSITION': 'VEC3', 'NORMAL': 'VEC3', 'TEXCOORD_0': 'VEC2', 'TEXCOORD_1': 'VEC2', 'TEXCOORD_2': 'VEC2', 'JOINTS_0': 'VEC4', 'WEIGHTS_0': 'VEC4'}
    part['attributes'] = {key: add_accessor(doc, raw, value, types[key], 5123 if key == 'JOINTS_0' else 5126) for key, value in result.items()}
    part['indices'] = add_accessor(doc, raw, new_faces.reshape(-1, 1), 'SCALAR', 5125)
    material = doc['materials'][part['material']]
    material['pbrMetallicRoughness']['metallicRoughnessTexture']['texCoord'] = 2
    if 'occlusionTexture' in material:
        material['occlusionTexture']['texCoord'] = 2
    return {'remapped_triangles': int(selected.sum()), 'duplicated_vertices': len(parents) - len(points)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--prepare', action='store_true')
    args = parser.parse_args()
    BEFORE.mkdir(parents=True, exist_ok=True)
    for name in ['base_custom.glb', 'base_custom_preview.glb', 'face_custom.glb', 'custom-face-fitting.blend', 'fitting.json']:
        if not (BEFORE / name).exists():
            shutil.copy2(OUT / name, BEFORE / name)
    doc, binary = read_glb(BEFORE / 'base_custom.glb')
    part, texture, image, original = color_image(doc, binary)
    points = accessor(doc, binary, part['attributes']['POSITION'])
    uv = accessor(doc, binary, part['attributes']['TEXCOORD_' + str(texture.get('texCoord', 0))])
    faces = accessor(doc, binary, part['indices']).reshape(-1, 3)
    if args.prepare:
        prepare(points, faces, uv, original)
        return
    edited = np.asarray(Image.open(WORK / 'edited-eyes-flat.png').convert('RGB').resize((1024, 512), Image.Resampling.LANCZOS), dtype=float)
    placement = json.loads((WORK / 'sampling-offsets.json').read_text())
    offsets = np.asarray(placement['sampling_offsets_px'])
    height, width = original.shape[:2]
    weights = np.zeros((height, width), dtype=float)
    replacement = original.astype(float)
    fitting = json.loads((BEFORE / 'fitting.json').read_text())
    tiles = fitting['lower_eyelid_fit']['secondary_uv_patches']
    for tile in tiles:
        side, size = tile['side'], tile['size']
        ox, oy = tile['origin']
        xx, yy = np.meshgrid(np.arange(size), np.arange(size))
        x, y = xx.ravel() + ox, yy.ravel() + oy
        position = np.column_stack([(xx.ravel() + .5) / size * .060 + (side * .033 - .0008) - .030,
                                    1.681 + .020 - (yy.ravel() + .5) / size * .040,
                                    np.full(size * size, .065)])
        weight = iris_mask(position)
        take = weight > 0
        x, y, position, weight = x[take], y[take], position[take], weight[take]
        sx = (position[:, 0] - (side * .033 - .0008)) / .026 * 512 + 256 - .5 + (0 if side < 0 else 512)
        sy = (1.6846 - position[:, 1]) / .026 * 512 + 256 - .5
        sx += offsets[int(side > 0), 0]
        sy += offsets[int(side > 0), 1]
        replacement[y, x] = np.column_stack([map_coordinates(edited[..., c], [sy, sx], order=1, mode='nearest') for c in range(3)])
        weights[y, x] = weight
    result = np.round(original * (1 - weights[..., None]) + replacement * weights[..., None]).astype(np.uint8)
    np.testing.assert_array_equal(result[weights == 0], original[weights == 0])
    changed = int(np.any(result != original, axis=2).sum())
    assert 0 < changed < width * height * .01
    image = Image.fromarray(result)
    image.save(WORK / 'face-albedo-iris-clean.png')
    Image.fromarray(np.round(weights * 255).astype(np.uint8)).save(WORK / 'iris-mask.png')
    stream = io.BytesIO()
    image.save(stream, format='PNG')
    mapping = None
    for name in ['base_custom.glb', 'face_custom.glb']:
        target_doc, baseline = read_glb(BEFORE / name)
        target_part, _, target_image, _ = color_image(target_doc, baseline)
        raw = bytearray(baseline)
        mapping = map_iris_faces(target_doc, baseline, raw, target_part, tiles, width, height)
        raw.extend(b'\0' * (-len(raw) % 4))
        target_doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(stream.getvalue())})
        original_images = [view_bytes(target_doc, baseline, img['bufferView']) for img in target_doc['images']]
        target_doc['images'][target_image] = {'bufferView': len(target_doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': 'Original hazel irises with coherent pupils and baked highlights removed'}
        raw.extend(stream.getvalue())
        for i, img in enumerate(target_doc['images']):
            if i != target_image:
                assert view_bytes(target_doc, raw, img['bufferView']) == original_images[i]
        write_glb(OUT / name, target_doc, RIDGE.FIT.compact(target_doc, raw))
    record = {'date': '2026-11-08', 'baseline_commit': '9f58038c', 'baseline_sha256': digest(BEFORE / 'base_custom.glb'),
              'method': 'Retouch hazel irises in existing UV1 tiles; include tiny original iris UV fragments and preserve ORM coordinates on UV2',
              'changed_albedo_texels': changed, 'triangle_positions_normals_uv0_weights_rig_animations_exact': True,
              'color_uv_mapping': mapping, 'orm_sampling_coordinates_preserved_on_uv2': True,
              'outside_iris_mask_exact': True, 'normal_orm_lash_rgba_bytes_exact': True,
              'source_projection_sha256': digest(WORK / 'original-eyes-flat.png'), 'edited_projection_sha256': digest(WORK / 'edited-eyes-flat.png'),
              'albedo_sha256': digest(WORK / 'face-albedo-iris-clean.png'), 'mask_sha256': digest(WORK / 'iris-mask.png'),
              'pupil_placement': placement,
              'tool': 'Built-in ImageGen', 'subscription': 'ChatGPT Pro 20x',
              'license': 'Retouch of the user-provided Tripo texture. Original license not supplied; source conditions apply.',
              'files': {name: digest(OUT / name) for name in ['base_custom.glb', 'face_custom.glb']}}
    (WORK / 'build.json').write_text(json.dumps(record, indent=2) + '\n')
    fitting['iris_texture_cleanup_v2'] = record
    fitting['status'] = 'Accepted eyelid ridge geometry retained; only iris color texture retouched for coherent pupils and reduced baked reflections'
    fitting['files'].update(record['files'])
    for path in [OUT / 'fitting.json', ROOT / 'doc/assets/modular-female-custom-face-fitting-v1.json']:
        path.write_text(json.dumps(fitting, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
