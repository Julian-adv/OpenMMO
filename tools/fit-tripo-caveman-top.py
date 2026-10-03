"""Fit and skin the preserved Tripo shoulder pelt and necklace."""
import importlib.util
import json
import sys
from pathlib import Path

import numpy as np
from scipy import sparse
from scipy.sparse.csgraph import connected_components
from scipy.sparse.linalg import factorized
from scipy.spatial import cKDTree

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import view_bytes
from outfits.rogue_layers import compact_weights

spec = importlib.util.spec_from_file_location('modular_fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/caveman_tripo_top_v1'
REPORT = ROOT / 'doc/assets/modular-caveman-tripo-top-fitting-v1.json'


def main():
    source = OUTPUT / 'source.glb'
    base = ROOT / 'assets/modular_human_male_01/parts/fitted/base.glb'
    assert fit.digest(source) == '6332d1538f9db297dcc62f307a330b2853f8e9e11bd300d3a0014cf57f146e89'
    assert fit.digest(base) == 'ae72eb53953dd86b716859a402700eab536863e245c5261acb2592e5ef87ea5b'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    original = io.accessor(doc, raw, attrs['POSITION'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    uv = io.accessor(doc, raw, attrs['TEXCOORD_0'])
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    _, first, inverse = np.unique(np.round(original, 5), axis=0, return_index=True, return_inverse=True)
    source_points = original[first].astype(float)
    welded_faces = inverse[faces]
    edges = np.unique(np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]], welded_faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(first), len(first)))
    count, labels = connected_components(adjacency, directed=False)
    assert sorted(np.bincount(labels).tolist()) == [54, 117, 884]
    pelt = labels == labels[np.argmin(source_points[:, 0])]
    x, y, z = source_points.T
    horn = (~pelt) & (x < -.09) & (y > .535) & (y < .78) & (z < .235)
    shoulder = io.smoothstep((-x - .025) / .09) * (1 - io.smoothstep((z - .13) / .07))
    shoulder[pelt] = 1
    points = source_points * [.45, .35, .40] + [-.08325, 1.30, .005]
    points[:, 1] += .055 * shoulder
    points[:, 2] += (source_points[:, 2] * .10 - .015) * shoulder
    surface = fit.body_surface(('torso', 'neck', 'upper_arms'))
    laplacian = sparse.diags(np.asarray(adjacency.sum(1)).ravel()) - adjacency
    smooth = factorized((sparse.eye(len(first)) + .7 * laplacian).tocsc())
    iterations = []
    for step in range(24):
        nearest, normals, _ = fit.nearest_surface(points, surface, candidates=128)
        signed = np.sum((points - nearest) * normals, axis=1)
        correction = normals * np.clip(.009 - signed, 0, .012)[:, None]
        correction = np.column_stack([smooth(correction[:, i]) for i in range(3)])
        points += correction
        iterations.append(dict(step=step, minimum_signed_distance_m=float(signed.min()), maximum_correction_m=float(np.linalg.norm(correction, axis=1).max())))
    distance = cKDTree(source_points[horn]).query(source_points)[0]
    attachment = 1 - io.smoothstep(distance / .23)
    attachment[labels != labels[np.flatnonzero(horn)[0]]] = 0
    dense = np.zeros((len(points), len(fit.NAMES)))
    dense[:, fit.NAMES.index('Spine2')] = 1 - attachment
    dense[:, fit.NAMES.index('RightShoulder')] = attachment
    _, _, transferred = fit.nearest_surface(points[pelt], surface, candidates=128)
    permitted = ['Spine2', 'RightShoulder', 'RightArm']
    for name in permitted:
        dense[pelt, fit.NAMES.index(name)] = transferred[:, fit.NAMES.index(name)]
    trunk = [fit.NAMES.index(n) for n in ['Spine', 'Spine1', 'Neck']]
    dense[pelt, fit.NAMES.index('Spine2')] += transferred[:, trunk].sum(1)
    dense[pelt] /= dense[pelt].sum(1, keepdims=True)
    smooth_weights = factorized((sparse.eye(len(first)) + 1.5 * laplacian).tocsc())
    for bone in permitted:
        index = fit.NAMES.index(bone)
        dense[pelt, index] = smooth_weights(dense[:, index])[pelt]
    dense[pelt] /= dense[pelt].sum(1, keepdims=True)
    joints, weights = compact_weights(dense)
    positions = points[inverse]
    binary = bytearray(raw)
    attrs.update(io.add_skin_attributes(doc, binary, positions, io.smooth_normals(positions, faces), joints[inverse], weights[inverse]))
    attrs.pop('TANGENT', None)
    doc['meshes'][0]['name'] = 'top_caveman_tripo'
    fit.with_rig(doc, binary, 'top_caveman', 'tripo_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'top_caveman.glb'
    fit.write_glb(target, doc, binary)
    exported_attrs = doc['meshes'][0]['primitives'][0]['attributes']
    assert np.array_equal(uv, io.accessor(doc, binary, exported_attrs['TEXCOORD_0']))
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    samples = np.concatenate([positions, positions[faces].mean(1)])
    near, normals, _ = fit.nearest_surface(samples, surface, candidates=128)
    distances = np.sum((samples - near) * normals, axis=1)
    report = dict(
        status='Fitted and rigged candidate; animation and visual review must match the exported hash',
        date='2026-10-04',
        source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
        base=dict(path=str(base.relative_to(ROOT)), sha256=fit.digest(base)),
        rig_id='human_male_01_mixamo_candidate_v2',
        method='Separate shoulder-cap alignment and necklace alignment, smooth body clearance, rigid chest ornaments and shoulder-surface skin transfer',
        source_triangles=len(faces), preserved_uv_and_embedded_texture=True,
        source_component_vertices=sorted(np.bincount(labels).tolist()), pelt_component_vertices=int(pelt.sum()),
        clearance_target_m=.009, correction_iterations=iterations,
        rest_surface_diagnostic=dict(samples=len(samples), minimum_signed_distance_m=float(distances.min()), negative_samples=int((distances < 0).sum())),
        weight_policy=dict(necklace='Spine2', shoulder_ornament='RightShoulder with cord attachment transition', pelt=permitted, maximum_influences=4),
        regions=dict(pelt_vertices=np.flatnonzero(pelt[inverse]).tolist(), horn_vertices=np.flatnonzero(horn[inverse]).tolist(), ornament_vertices=np.flatnonzero((~pelt & (attachment == 0))[inverse]).tolist()),
        validation=fit.validate(target),
        user_selection=dict(date='2026-10-04', reference='doc/images/characters/modular_human_male_01/parts/caveman/tripo-top-preferred-reference.png', shape_and_placement_preserved=True),
    )
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['validation', 'rest_surface_diagnostic']}, indent=2))


if __name__ == '__main__':
    main()
