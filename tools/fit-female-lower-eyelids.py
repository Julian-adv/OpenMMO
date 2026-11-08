import argparse
import hashlib
import io
import json
import shutil
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
WORK = OUT / 'lower_lid_fit_v1'
BEFORE = WORK / 'before'
KNOTS = [.016, .020, .026, .033, .040, .047, .052, .056]
BOTTOM = [1.6819, 1.6804, 1.6795, 1.6796, 1.6806, 1.6826, 1.6835, 1.6840]
DROP = [0, .0007, .0017, .0021, .0018, .0009, .0002, 0]
TOP = [1.6819, 1.6860, 1.6891, 1.6894, 1.6878, 1.6854, 1.6835, 1.6840]


def smooth(value):
    value = np.clip(value, 0, 1)
    return value * value * (3 - 2 * value)


def displacement(points, strength):
    x, y, z = points.T
    absolute = np.abs(x + .0008)
    bottom = np.interp(absolute, KNOTS, BOTTOM)
    delta = np.interp(absolute, KNOTS, DROP) * strength
    distance = y - bottom
    upper = 1 - smooth((distance - .0001) / .0006)
    lower = 1 - smooth((-distance - .0005) / .0060)
    depth = smooth((z - .048) / .009)
    top = np.interp(absolute, KNOTS, TOP)
    pin_upper = 1 - smooth((y - top + .0010) / .0005)
    return delta * upper * lower * depth * pin_upper


def repair_border_alpha(doc, raw, source_uv):
    part = next(mesh for mesh in doc['meshes'] if mesh['name'] == 'eyelashes_female_custom')['primitives'][0]
    uv = accessor(doc, raw, part['attributes']['TEXCOORD_0'])
    packed = accessor(doc, raw, part['attributes']['TEXCOORD_1'])
    faces = accessor(doc, raw, part['indices']).reshape(-1, 3)
    key = lambda values: b''.join(sorted(row.tobytes() for row in values.astype('<f4')))
    face = next(face for face in faces if key(uv[face]) == key(source_uv))
    texture = doc['materials'][part['material']]['pbrMetallicRoughness']['baseColorTexture']['index']
    image_id = doc['textures'][texture]['source']
    original = np.asarray(Image.open(io.BytesIO(view_bytes(doc, raw, doc['images'][image_id]['bufferView']))).convert('RGBA'))
    mask = Image.new('L', (original.shape[1], original.shape[0]))
    ImageDraw.Draw(mask).polygon([tuple(point) for point in packed[face] * [mask.width, mask.height] - .5], fill=255)
    mask = np.asarray(mask.filter(ImageFilter.MaxFilter(3))) > 0
    repaired = original.copy()
    repaired[mask, 3] = 255
    np.testing.assert_array_equal(original[..., :3], repaired[..., :3])
    image = Image.fromarray(repaired)
    image.save(WORK / 'eyelash-alpha-lid-fit.png')
    stream = io.BytesIO()
    image.save(stream, format='PNG')
    raw.extend(b'\0' * (-len(raw) % 4))
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(stream.getvalue())})
    doc['images'][image_id] = {'bufferView': len(doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': 'Original lash alpha with lower lid border restored'}
    raw.extend(stream.getvalue())
    return int(np.any(repaired != original, axis=2).sum())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--strength', type=float, default=1)
    args = parser.parse_args()
    assert 0 < args.strength <= 1.2
    BEFORE.mkdir(parents=True, exist_ok=True)
    for name in ['base_custom.glb', 'base_custom_preview.glb', 'face_custom.glb', 'custom-face-fitting.blend', 'fitting.json']:
        if not (BEFORE / name).exists():
            shutil.copy2(OUT / name, BEFORE / name)
    records = {}
    source, source_raw = read_glb(OUT / 'source.glb')
    part = source['meshes'][0]['primitives'][0]
    face = accessor(source, source_raw, part['indices']).reshape(-1, 3)[10464]
    source_uv = accessor(source, source_raw, part['attributes']['TEXCOORD_0'])[face]
    changed_alpha = 0
    for name in ['base_custom.glb', 'face_custom.glb']:
        doc, binary = read_glb(BEFORE / name)
        raw = bytearray(binary)
        changed_alpha = repair_border_alpha(doc, raw, source_uv)
        for mesh in doc['meshes']:
            for part in mesh['primitives']:
                attrs = part['attributes']
                position_id, normal_id = attrs['POSITION'], attrs['NORMAL']
                points = accessor(doc, binary, position_id)
                normals = accessor(doc, binary, normal_id)
                drop = displacement(points.astype(float), args.strength)
                active = drop > 1e-10
                absolute = np.abs(points[:, 0] + .0008)
                upper_lid = points[:, 1] >= np.interp(absolute, KNOTS, TOP) - .0005
                iris_center = np.hypot(absolute - .033, points[:, 1] - 1.6846) < .0018
                assert not active[upper_lid | iris_center].any()
                moved = points.copy()
                moved[active, 1] -= drop[active]
                jacobian = np.tile(np.eye(3), (int(active.sum()), 1, 1))
                for axis in range(3):
                    step = np.eye(3)[axis] * 1e-5
                    derivative = (displacement(points[active] + step, args.strength) - displacement(points[active] - step, args.strength)) / 2e-5
                    jacobian[:, 1, axis] -= derivative
                assert np.all(np.linalg.det(jacobian) > .35)
                updated = normals.copy()
                changed_normals = np.linalg.solve(jacobian.transpose(0, 2, 1), normals[active, :, None])[..., 0]
                changed_normals /= np.maximum(np.linalg.norm(changed_normals, axis=1, keepdims=True), 1e-12)
                updated[active] = changed_normals
                triangles = accessor(doc, binary, part['indices']).reshape(-1, 3)
                old = points[triangles]
                new = moved[triangles]
                old_cross = np.cross(old[:, 1] - old[:, 0], old[:, 2] - old[:, 0])
                new_cross = np.cross(new[:, 1] - new[:, 0], new[:, 2] - new[:, 0])
                valid = np.linalg.norm(old_cross, axis=1) > 1e-12
                assert np.all(np.einsum('ij,ij->i', old_cross[valid], new_cross[valid]) > 0)
                for index, values in [(position_id, moved), (normal_id, updated)]:
                    a = doc['accessors'][index]
                    view = doc['bufferViews'][a['bufferView']]
                    target = np.ndarray(values.shape, dtype='<f4', buffer=raw,
                                        offset=view.get('byteOffset', 0) + a.get('byteOffset', 0),
                                        strides=(view.get('byteStride', values.shape[1] * 4), 4))
                    target[:] = values
                    a['min'] = values.min(0).tolist()
                    a['max'] = values.max(0).tolist()
                np.testing.assert_array_equal(moved[~active], points[~active])
                np.testing.assert_array_equal(moved[:, [0, 2]], points[:, [0, 2]])
                for semantic, index in attrs.items():
                    if semantic not in ['POSITION', 'NORMAL']:
                        np.testing.assert_array_equal(accessor(doc, raw, index), accessor(doc, binary, index))
                np.testing.assert_array_equal(accessor(doc, raw, part['indices']), triangles.reshape(-1, 1))
                if name == 'base_custom.glb':
                    records[mesh['name']] = {'moved_vertices': int(active.sum()), 'maximum_drop_mm': float(drop.max(initial=0) * 1000),
                                            'minimum_local_jacobian': float(np.linalg.det(jacobian).min(initial=1))}
                if mesh['name'] == 'female_body':
                    assert not active.any()
        doc['buffers'][0]['byteLength'] = len(raw)
        write_glb(OUT / name, doc, raw)
    baseline = json.loads((BEFORE / 'fitting.json').read_text())
    record = {'date': '2026-11-08', 'method': 'Local downward deformation of lower eyelid fold toward the painted sclera edge; continuous across coincident vertices',
              'strength': args.strength, 'absolute_x_knots_m': KNOTS, 'original_lid_y_m': BOTTOM,
              'upper_transition_m': [.0001, .0007], 'lower_transition_m': [-.0065, -.0005],
              'drop_knots_m': [value * args.strength for value in DROP], 'meshes': records,
              'baseline_path': 'lower_lid_fit_v1/before/base_custom.glb',
              'baseline_sha256': hashlib.sha256((BEFORE / 'base_custom.glb').read_bytes()).hexdigest(),
              'head_unchanged_outside_local_lower_eyelid_region': True, 'body_and_neck_unchanged': True,
              'horizontal_and_depth_coordinates_unchanged': True, 'color_normal_orm_uv_weights_rig_and_animations_unchanged': True,
              'lower_lid_alpha_repair': {'source_faces': [10464], 'changed_alpha_texels': changed_alpha, 'original_rgb_unchanged': True},
              'triangle_indices_and_winding_preserved': True, 'new_triangles': 0,
              'upper_lids_and_iris_centers_unchanged': True}
    baseline.update(date='2026-11-08', new_head_scale_and_translation_only=False, original_head_geometry_preserved=False,
                    geometry_rig_and_animations_unchanged=False, lower_eyelid_fit=record,
                    head_affine_scope='Original scale and translation except the localized lower eyelid fit',
                    status='Original neck fit, lash alpha and iris cleanup; lower eyelid fold lowered to the painted sclera edge')
    baseline['original_fit_maximum_head_affine_error_m'] = baseline['maximum_head_affine_error_m']
    baseline['maximum_head_affine_error_m'] = records['face_female_custom']['maximum_drop_mm'] / 1000
    baseline['maximum_exported_head_affine_error_m'] = baseline['maximum_head_affine_error_m']
    baseline['eyelashes']['positions_normals_weights_and_triangles_preserved'] = False
    baseline['eyelashes']['weights_uv_triangles_and_color_preserved'] = True
    baseline['eyelashes']['position_and_normal_change_scope'] = 'Local lower eyelid fit'
    baseline['files'].update({name: hashlib.sha256((OUT / name).read_bytes()).hexdigest() for name in ['base_custom.glb', 'face_custom.glb']})
    for path in [OUT / 'fitting.json', ROOT / 'doc/assets/modular-female-custom-face-fitting-v1.json']:
        path.write_text(json.dumps(baseline, indent=2) + '\n')
    (WORK / 'build.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
