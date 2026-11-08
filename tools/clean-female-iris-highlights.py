import argparse
import copy
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
from lib.glb_accessors import accessor
from lib.female_custom_eyes import eye_weight, smoothstep


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
WORK = OUT / 'iris_highlight_cleanup_v1'
BEFORE = WORK / 'before'
SPEC = importlib.util.spec_from_file_location('female_fitter', ROOT / 'tools/fit-female-custom-face.py')
FIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIT)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def raster(polygon, width, height):
    low = np.maximum(np.floor(polygon.min(0)).astype(int), 0)
    high = np.minimum(np.ceil(polygon.max(0)).astype(int), [width - 1, height - 1])
    matrix = np.column_stack((polygon[1] - polygon[0], polygon[2] - polygon[0]))
    if np.any(high < low) or abs(np.linalg.det(matrix)) < 1e-10:
        return None
    xx, yy = np.meshgrid(np.arange(low[0], high[0] + 1), np.arange(low[1], high[1] + 1))
    pixels = np.column_stack((xx.ravel() + .5, yy.ravel() + .5))
    local = (pixels - polygon[0]) @ np.linalg.inv(matrix).T
    bary = np.column_stack((1 - local.sum(1), local))
    inside = bary.min(1) >= -1e-6
    return xx.ravel()[inside], yy.ravel()[inside], bary[inside]


def prepare(points, faces, uv, original):
    height, width = original.shape[:2]
    canvas = np.full((512, 1024, 3), [120, 90, 55], dtype=np.uint8)
    for side, offset in [(-1, 0), (1, 512)]:
        xy = (points[:, :2] - [side * .033 - .0008, 1.6846]) / .026 * 512 + 256
        xy[:, 1] = 512 - xy[:, 1]
        depth = np.full((512, 512), -np.inf)
        for face in faces:
            if points[face, 2].max() < .052:
                continue
            sample = raster(xy[face], 512, 512)
            if sample is None:
                continue
            x, y, bary = sample
            z = bary @ points[face, 2]
            take = z > depth[y, x]
            x, y, z, bary = x[take], y[take], z[take], bary[take]
            pixels = np.clip((bary @ uv[face] * [width, height]).astype(int), [0, 0], [width - 1, height - 1])
            canvas[y, x + offset] = original[pixels[:, 1], pixels[:, 0]]
            depth[y, x] = z
    Image.fromarray(canvas).save(WORK / 'original-eyes-flat.png')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--strength', type=float, default=.65)
    args = parser.parse_args()
    assert 0 < args.strength <= .75
    BEFORE.mkdir(parents=True, exist_ok=True)
    for name in ['base_custom.glb', 'base_custom_preview.glb', 'face_custom.glb', 'custom-face-fitting.blend', 'fitting.json']:
        if not (BEFORE / name).exists():
            shutil.copy2(OUT / name, BEFORE / name)
    doc, baseline = read_glb(BEFORE / 'base_custom.glb')
    primitive = doc['meshes'][1]['primitives'][0]
    points = accessor(doc, baseline, primitive['attributes']['POSITION'])
    uv = accessor(doc, baseline, primitive['attributes']['TEXCOORD_0'])
    faces = accessor(doc, baseline, primitive['indices']).reshape(-1, 3)
    texture = doc['materials'][primitive['material']]['pbrMetallicRoughness']['baseColorTexture']['index']
    image_index = doc['textures'][texture]['source']
    original = np.asarray(Image.open(io.BytesIO(view_bytes(doc, baseline, doc['images'][image_index]['bufferView']))).convert('RGB'))
    if args.prepare:
        Image.fromarray(original).save(WORK / 'original-face-albedo.png')
        prepare(points, faces, uv, original)
        return
    edited = np.asarray(Image.open(WORK / 'edited-eyes-flat.png').convert('RGB').resize((1024, 512), Image.Resampling.LANCZOS), dtype=float)
    height, width = original.shape[:2]
    weights = np.zeros((height, width), dtype=float)
    correction = np.zeros_like(original, dtype=float)
    triangle_ids = []
    for i, face in enumerate(faces):
        center = points[face].mean(0)
        if abs(abs(center[0] + .0008) - .033) > .010 or abs(center[1] - 1.6846) > .010 or center[2] < .060:
            continue
        sample = raster(uv[face] * [width, height], width, height)
        if sample is None:
            continue
        x, y, bary = sample
        position = bary @ points[face]
        side = np.where(position[:, 0] < -.0008, -1, 1)
        dx = position[:, 0] - (side * .033 - .0008)
        dy = position[:, 1] - 1.6846
        radial = np.hypot(dx, dy)
        iris = 1 - smoothstep((radial - .0066) / .0012)
        aperture = eye_weight(position)
        sx = (dx / .026 + .5) * 512 - .5 + np.where(side < 0, 0, 512)
        sy = (.5 - dy / .026) * 512 - .5
        target = np.column_stack([map_coordinates(edited[..., channel], [sy, sx], order=1, mode='nearest') for channel in range(3)])
        source = original[y, x].astype(float)
        source_light = source @ [.2126, .7152, .0722]
        target_light = target @ [.2126, .7152, .0722]
        highlight = smoothstep((source_light - 125) / 60)
        reduction = smoothstep((source_light - target_light - 8) / 24)
        weight = iris * aperture * highlight * reduction * args.strength
        take = weight > weights[y, x]
        x, y, weight = x[take], y[take], weight[take]
        correction[y, x] = np.clip(target[take] - source[take], -100, 12)
        weights[y, x] = weight
        if np.any(weight > .01):
            triangle_ids.append(i)
    result = np.round(np.clip(original.astype(float) + correction * weights[..., None], 0, 255)).astype(np.uint8)
    changed = np.any(result != original, axis=2)
    assert 0 < changed.sum() < original.shape[0] * original.shape[1] * .005
    assert np.array_equal(result[weights == 0], original[weights == 0])
    output = Image.fromarray(result)
    output.save(WORK / 'face-albedo-iris-clean.png')
    Image.fromarray(np.round(weights / args.strength * 255).astype(np.uint8)).save(WORK / 'iris-highlight-mask.png')
    stream = io.BytesIO()
    output.save(stream, format='PNG')
    for name in ['base_custom.glb', 'face_custom.glb']:
        target_doc, original_raw = read_glb(BEFORE / name)
        untouched = copy.deepcopy(target_doc)
        face = next(mesh for mesh in target_doc['meshes'] if mesh['name'] == 'face_female_custom')['primitives'][0]
        color_texture = target_doc['materials'][face['material']]['pbrMetallicRoughness']['baseColorTexture']['index']
        target_image = target_doc['textures'][color_texture]['source']
        raw = bytearray(original_raw)
        raw.extend(b'\0' * (-len(raw) % 4))
        target_doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(stream.getvalue())})
        target_doc['images'][target_image] = {'bufferView': len(target_doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': 'Original face with subdued baked iris highlights'}
        raw.extend(stream.getvalue())
        assert target_doc['meshes'] == untouched['meshes']
        assert target_doc['materials'] == untouched['materials']
        assert target_doc['nodes'] == untouched['nodes']
        assert target_doc.get('skins') == untouched.get('skins')
        assert target_doc.get('animations') == untouched.get('animations')
        for i, image in enumerate(untouched['images']):
            if i != target_image:
                assert view_bytes(target_doc, raw, target_doc['images'][i]['bufferView']) == view_bytes(untouched, original_raw, image['bufferView'])
        for mesh in untouched['meshes']:
            for part in mesh['primitives']:
                for index in [*part['attributes'].values(), part['indices']]:
                    np.testing.assert_array_equal(accessor(target_doc, raw, index), accessor(untouched, original_raw, index))
        write_glb(OUT / name, target_doc, FIT.compact(target_doc, raw))
    record = {'date': '2026-11-08', 'baseline_commit': 'f1775cb9', 'baseline_sha256': digest(BEFORE / 'base_custom.glb'),
              'method': 'ImageGen highlight retouch blended only into bright existing iris texels; maximum strength .65 and channel delta 65/255',
              'strength': args.strength, 'changed_albedo_pixels': int(changed.sum()), 'maximum_channel_change': int(np.abs(result.astype(int) - original).max()),
              'geometry_uv_weights_rig_animations_materials_and_lash_alpha_unchanged': True,
              'outside_highlight_mask_unchanged': True, 'normal_and_orm_images_unchanged': True,
              'source_projection_sha256': digest(WORK / 'original-eyes-flat.png'), 'edited_projection_sha256': digest(WORK / 'edited-eyes-flat.png'),
              'output_albedo_sha256': digest(WORK / 'face-albedo-iris-clean.png'), 'mask_sha256': digest(WORK / 'iris-highlight-mask.png'),
              'affected_head_triangle_ids': triangle_ids,
              'generator': 'Built-in ImageGen', 'subscription': 'ChatGPT Pro 20x',
              'prompt': 'Keep existing golden brown irises, pupil shapes, eye shapes, framing and coarse texture detail; reduce only baked bright ivory iris reflections with neighboring brown/gold. Leave sclera, lids and skin unchanged; do not redesign or sharpen.',
              'license': 'User-provided Tripo source; license not supplied. Retouch generated from its projected albedo on 2026-11-08.',
              'files': {name: digest(OUT / name) for name in ['base_custom.glb', 'face_custom.glb']}}
    (WORK / 'build.json').write_text(json.dumps(record, indent=2) + '\n')
    fitting = json.loads((BEFORE / 'fitting.json').read_text())
    fitting['source_texture_bytes_preserved'] = False
    fitting['original_head_alpha_only'] = False
    fitting['original_head_geometry_preserved'] = True
    fitting['iris_highlight_cleanup'] = {key: value for key, value in record.items() if key != 'affected_head_triangle_ids'}
    fitting['status'] = 'Original neck-fitted head and lash alpha; only baked iris highlights reduced'
    fitting['files'].update(record['files'])
    for path in [OUT / 'fitting.json', ROOT / 'doc/assets/modular-female-custom-face-fitting-v1.json']:
        path.write_text(json.dumps(fitting, indent=2) + '\n')
    print(json.dumps({key: value for key, value in record.items() if key != 'affected_head_triangle_ids'}))


if __name__ == '__main__':
    main()
