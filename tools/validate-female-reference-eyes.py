import hashlib
import importlib.util
import io
import json
from pathlib import Path

import numpy as np
from PIL import Image

from lib.glb import read_glb, view_bytes
from lib.glb_accessors import accessor


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
WORK = OUT / 'reference_v2'


def primitive(doc, raw, mesh):
    item = doc['meshes'][mesh]['primitives'][0]
    return {key: accessor(doc, raw, index) for key, index in item['attributes'].items()}, accessor(doc, raw, item['indices'])


def main():
    doc, raw = read_glb(OUT / 'base_custom.glb')
    baseline, baseline_raw = read_glb(WORK / 'before-reference.glb')
    body, indices = primitive(doc, raw, 0)
    old_body, old_indices = primitive(baseline, baseline_raw, 0)
    assert body.keys() == old_body.keys()
    for key in body:
        np.testing.assert_array_equal(body[key], old_body[key])
    np.testing.assert_array_equal(indices, old_indices)
    assert doc['nodes'] == baseline['nodes']
    for left, right in zip(doc['skins'], baseline['skins'], strict=True):
        assert {k: v for k, v in left.items() if k != 'inverseBindMatrices'} == {k: v for k, v in right.items() if k != 'inverseBindMatrices'}
        np.testing.assert_array_equal(accessor(doc, raw, left['inverseBindMatrices']), accessor(baseline, baseline_raw, right['inverseBindMatrices']))
    assert len(doc['animations']) == len(baseline['animations']) == 9
    for left, right in zip(doc['animations'], baseline['animations'], strict=True):
        assert left['channels'] == right['channels']
        for a, b in zip(left['samplers'], right['samplers'], strict=True):
            assert a.get('interpolation') == b.get('interpolation')
            for key in ['input', 'output']:
                np.testing.assert_array_equal(accessor(doc, raw, a[key]), accessor(baseline, baseline_raw, b[key]))
    original = np.load(WORK / 'original-face.npz')
    spec = importlib.util.spec_from_file_location('refine', ROOT / 'tools/refine-female-reference-eyes.py')
    refine = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(refine)
    original_attrs = {key: original[key] for key in original.files if key != 'faces'}
    recessed_face = refine.recess_original_eye_surface(original_attrs)
    prepared = {**recessed_face, **{key: recessed_face[key].astype(np.float64) for key in ['POSITION', 'NORMAL']}}
    expected_face = refine.warp_eye_region(prepared)
    face, indices = primitive(doc, raw, 1)
    count = len(original['POSITION'])
    jacobian = np.tile(np.eye(3), (count, 1, 1))
    for axis in range(3):
        delta = np.eye(3)[axis] * 1e-5
        jacobian[:, :, axis] += (refine.eye_displacement(prepared['POSITION'] + delta) - refine.eye_displacement(prepared['POSITION'] - delta)) / 2e-5
    minimum_jacobian = float(np.linalg.det(jacobian).min())
    assert minimum_jacobian > 0
    for key in ['POSITION', 'NORMAL', 'JOINTS_0', 'WEIGHTS_0']:
        np.testing.assert_array_equal(face[key][:count], expected_face[key].astype(face[key].dtype))
    offset = np.linalg.norm(face['POSITION'][:count] - original['POSITION'], axis=1)
    recessed = np.linalg.norm(recessed_face['POSITION'] - original['POSITION'], axis=1) > 0
    outside = (np.abs(refine.eye_offset(original['POSITION'])) <= 1e-10) & ~recessed
    for key in ['POSITION', 'NORMAL']:
        np.testing.assert_array_equal(face[key][:count][outside], original[key][outside])
    assert offset.max() < .025
    np.testing.assert_array_equal(indices[:original['faces'].size].reshape(-1, 3), original['faces'])
    uv_error = float(np.abs(face['TEXCOORD_0'][:count] * [2048, 2576] - original['TEXCOORD_0'] * 2048).max())
    assert uv_error < .001
    cap_ids = []
    for eye in range(2):
        start = count + eye * (49 * 9 + 49 * 2)
        cap_ids.extend(range(start, start + 49 * 9))
        cap_edge = face['POSITION'][start:start + 49 * 9:9]
        rim_edge = face['POSITION'][start + 49 * 9:start + 49 * 9 + 49 * 2:2]
        np.testing.assert_array_equal(cap_edge, rim_edge)
    source, source_raw = read_glb(OUT / 'source.glb')
    texture = source['materials'][0]['pbrMetallicRoughness']['baseColorTexture']['index']
    entry = source['images'][source['textures'][texture]['source']]
    source_color = np.asarray(Image.open(io.BytesIO(view_bytes(source, source_raw, entry['bufferView']))).convert('RGB'))
    color = np.asarray(Image.open(WORK / 'face-reference-albedo.png'))
    np.testing.assert_array_equal(color[:2048], source_color)
    triangles = []
    for mesh in range(3):
        attrs, mesh_indices = primitive(doc, raw, mesh)
        for value in attrs.values():
            assert np.isfinite(value).all()
        assert np.allclose(np.linalg.norm(attrs['NORMAL'], axis=1), 1, atol=1e-5)
        assert np.allclose(attrs['WEIGHTS_0'].sum(1), 1, atol=2e-6)
        triangles.append(mesh_indices.size // 3)
    points = face['POSITION'][:count][original['faces']]
    samples = np.concatenate([points, points.mean(1, keepdims=True)], axis=1)
    x = np.abs(samples[..., 0] + .0008)
    sides = np.where(samples[..., 0] + .0008 < 0, -1, 1)
    u = refine.eye_parameter(x, sides)
    canonical_x = refine.EYE_INNER + refine.EYE_WIDTH * u
    bottom, top = refine.BOTTOM(canonical_x), refine.TOP(canonical_x)
    bottom += refine.eye_offset(np.stack([samples[..., 0], bottom, refine.BOTTOM_Z(canonical_x)], axis=-1))
    top += refine.eye_offset(np.stack([samples[..., 0], top, refine.TOP_Z(canonical_x)], axis=-1))
    v = (samples[..., 1] - bottom) / np.maximum(top - bottom, 1e-9)
    inside = (u > .001) & (u < .999) & (v > .001) & (v < .999)
    cap_surface = refine.surface(np.clip(u, .0001, .9999), np.clip(v, 0, 1), sides)
    z = (cap_surface + refine.eye_displacement(cap_surface))[..., 2]
    covered = int((inside & (samples[..., 2] < z - .0001)).all(1).sum())
    report = {
        'date': '2026-11-07', 'sha256': hashlib.sha256((OUT / 'base_custom.glb').read_bytes()).hexdigest(),
        'body_attributes_and_indices_exact': True, 'bone_nodes_and_bind_matrices_exact': True,
        'animation_tracks_exact': 9, 'original_head_triangles_joints_and_weights_exact': True,
        'original_head_attributes_exact_outside_eye_region': True,
        'local_eye_region_deformed_vertices': int((offset > 0).sum()),
        'maximum_eye_region_displacement_m': float(offset.max()),
        'original_eye_surface_recessed_vertices': int(recessed.sum()),
        'maximum_eye_region_y_displacement_m': float(np.abs(face['POSITION'][:count, 1] - original['POSITION'][:, 1]).max()),
        'minimum_eye_deformation_jacobian': minimum_jacobian,
        'lower_eyelid_to_eye_surface_join_error_m': 0,
        'original_skin_albedo_pixels_exact': True, 'maximum_original_uv_pixel_error': uv_error,
        'unit_normals_and_normalized_weights': True, 'triangles_by_mesh': triangles,
        'assembled_triangles_including_hidden': sum(triangles),
        'known_original_eye_triangles_covered_in_front_view': covered,
        'front_view_visible_geometry_upper_bound': sum(triangles) - covered,
        'visibility_scope': 'Conservative triangle estimate using all corners and centroid behind the new caps. Other backfaces, hair occlusion and partial coverage are not subtracted.',
    }
    before_path = WORK / 'depth_refinement/before.glb'
    if before_path.exists():
        before_doc, before_raw = read_glb(before_path)
        before_face, _ = primitive(before_doc, before_raw, 1)
        old_z = before_face['POSITION'][cap_ids, 2]
        new_z = face['POSITION'][cap_ids, 2]
        report['depth_refinement'] = {
            'before_sha256': hashlib.sha256(before_path.read_bytes()).hexdigest(),
            'maximum_cap_retreat_m': float((old_z - new_z).max()),
            'median_cap_retreat_m': float(np.median(old_z - new_z)),
            'maximum_cap_edge_advance_m': float(max(0, (new_z - old_z).max())),
            'old_foremost_cap_z_m': float(old_z.max()), 'new_foremost_cap_z_m': float(new_z.max()),
            'foremost_cap_retreat_m': float(old_z.max() - new_z.max()),
        }
    (WORK / 'geometry-validation.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
