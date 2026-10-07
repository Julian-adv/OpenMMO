import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy.interpolate import PchipInterpolator
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components
from scipy.spatial import cKDTree

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/face_tripo_ranger_v1'
spec = importlib.util.spec_from_file_location('face', ROOT / 'tools/fit-tripo-rugged-face.py')
face = importlib.util.module_from_spec(spec)
spec.loader.exec_module(face)
face.OUTPUT = OUTPUT
face.FACE_NAME = 'face_ranger'
face.FIT_VERSION = 'ranger_face_v1'
fit, io = face.fit, face.io


def fit_scalp(points, normals, faces):
    original = points.copy()
    _, welded = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    edges = welded[faces[:, [0, 1, 1, 2, 2, 0]]].reshape(-1, 2)
    adjacency = coo_matrix((np.ones(len(edges)), (edges[:, 0], edges[:, 1])), shape=(welded.max() + 1,) * 2)
    _, labels = connected_components(adjacency, directed=False)
    body = labels[welded] == np.argmax(np.bincount(labels[welded]))
    crown = io.smoothstep((original[:, 1] - 1.80) / .03)
    rear = io.smoothstep((original[:, 1] - 1.72) / .055) * (1 - io.smoothstep((original[:, 2] + .065) / .04))
    influence = np.maximum(crown, rear) * body
    ear_distance = cKDTree(original[~body]).query(original)[0]
    influence *= io.smoothstep((ear_distance - .006) / .014)
    surface = fit.body_surface(('head',))
    nearest, normal, _ = fit.nearest_surface(points, surface)
    signed_before = np.einsum('ij,ij->i', points - nearest, normal)
    correction = np.maximum(signed_before + .001, 0) * influence
    points = points - normal * correction[:, None]
    for _ in range(3):
        nearest, normal, _ = fit.nearest_surface(points, surface)
        signed = np.einsum('ij,ij->i', points - nearest, normal)
        correction = np.maximum(signed + .001, 0) * (influence > .999)
        points -= normal * correction[:, None]
    displacement = np.linalg.norm(points - original, axis=1)
    changed_faces = faces[np.any(displacement[faces] > 1e-8, axis=1)]
    affected = np.unique(changed_faces)
    normals[affected] = io.smooth_normals(points, faces)[affected]
    samples = np.concatenate([points, points[faces].mean(1)])
    covered = np.concatenate([influence > .999, np.min(influence[faces], axis=1) > .999])
    nearest, normal, _ = fit.nearest_surface(samples[covered], surface)
    signed_after = np.einsum('ij,ij->i', samples[covered] - nearest, normal)
    assert signed_after.max() < 1e-5, float(signed_after.max() * 1000)
    assert np.array_equal(points[influence == 0], original[influence == 0])
    return points, normals, dict(
        method='Taper only crown and rear scalp inside the canonical head by 1mm; separate ear components and facial vertices outside the scalp mask preserved',
        adjusted_vertices=int((displacement > 1e-8).sum()), maximum_displacement_mm=float(displacement.max() * 1000),
        maximum_outward_distance_before_mm=float(signed_before[influence > .999].max() * 1000),
        maximum_outward_distance_after_mm=float(signed_after.max() * 1000),
        vertex_and_triangle_center_samples=int(covered.sum()), face_and_ear_positions_outside_mask_preserved=True,
        crop_hair_sha256=fit.digest(io.PARTS / 'fitted/hair_crop.glb'))


def main():
    path = OUTPUT / 'source.glb'
    assert fit.digest(path) == '6a2558b17963363726f4a798b6b0bd2fabd6c8f242597487100fda0a8223d4d8'
    source, raw = fit.read_glb(path)
    primitive = source['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    original = io.accessor(source, raw, attrs['POSITION'])
    uv = io.accessor(source, raw, attrs['TEXCOORD_0'])
    normals = io.accessor(source, raw, attrs['NORMAL'])
    faces = io.accessor(source, raw, primitive['indices']).reshape(-1, 3)
    curve = PchipInterpolator([.18, .55, .99951171875], [1.663, 1.7825, 1.9])
    points = original * [.30, 1, .29] + [0, 0, .007]
    points[:, 1] = curve(original[:, 1])
    normals = io.unit(normals / np.column_stack([np.full(len(points), .30), curve.derivative()(original[:, 1]), np.full(len(points), .29)]))
    points, normals, scalp = fit_scalp(points, normals, faces)
    cut = 1.706 - .06 * io.smoothstep((points[:, 2] + .01) / .06) + np.maximum(np.abs(points[:, 0]) - .065, 0) * .4
    points, uv, normals, faces = face.clip(points, uv, normals, faces, points[:, 1] - cut)
    result = face.build_face(source, raw, points, uv, normals, faces)
    report = dict(date='2026-10-08',
        source=dict(path=str(path.relative_to(ROOT)), sha256=fit.digest(path),
            original_path=r'Y:\public\web_downloads\bald+male+head+3d+model.glb', mounted_path='/mnt/y/web_downloads/bald+male+head+3d+model.glb',
            provider='User-supplied Tripo output', received_date='2026-10-08', generation_date=None,
            subscription_tier='Unconfirmed for this file', license='Original Tripo output terms; see doc/assets/characters.md'),
        base=dict(path=str((io.PARTS / 'fitted/base.glb').relative_to(ROOT)), sha256=fit.digest(io.PARTS / 'fitted/base.glb')),
        source_triangles=4026, **result, canonical_rig_preserved=True, original_face_texture_bytes_preserved=True,
        scalp_fitting=scalp,
        transform=dict(horizontal_scale=[.30, .29], depth_offset_m=.007,
            source_y_landmarks=[.18, .55, .99951171875], target_y_landmarks_m=[1.663, 1.7825, 1.9],
            neck_cut_front_y_m=1.646, neck_cut_back_y_m=1.706),
        neck_surface=dict(retained_canonical_lower_neck=True, exact_seam_weight_interpolation=True,
            shared_boundary_normals=True, texture='512px projection blending body and head skin; original face JPEG preserved'),
        files={name:fit.digest(OUTPUT / name) for name in ['face_ranger.glb', 'base_ranger.glb', 'neck-transition.png', 'neck-seam-validation.json']},
        validations=[fit.validate(OUTPUT / name) for name in ['face_ranger.glb', 'base_ranger.glb']],
        scope='Fitting candidate; canonical body attributes except upper-neck normals preserved. Facial expressions and all equipment intersections require separate validation.')
    (ROOT / 'doc/assets/modular-ranger-tripo-face-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
