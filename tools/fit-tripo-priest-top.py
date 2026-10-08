"""Fit the preserved priest robe without assigning its hem to leg bones."""
import importlib.util
import json
import sys
from pathlib import Path

import numpy as np
from scipy import sparse
from scipy.sparse.csgraph import connected_components
from scipy.sparse.linalg import factorized
from scipy.spatial import ConvexHull

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import view_bytes
from outfits.rogue_layers import compact_weights

spec = importlib.util.spec_from_file_location('modular_fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/priest_tripo_top_v1'


def align(original):
    points = original * [1.14, 1.16, 1.12] + [0, .48, -.05]
    lower = original[:, 1] < .43
    points[lower, 1] = .64 + original[lower, 1] * ((.48 + .43 * 1.16 - .64) / .43)
    edge = np.interp(original[:, 1], [.42, .65, .80, .95], [.22, .18, .12, .13])
    sleeve = io.smoothstep((abs(original[:, 0]) - edge) / .065)
    sleeve *= io.smoothstep((original[:, 1] - .35) / .05)
    for side, sign in [('Left', 1), ('Right', -1)]:
        start = np.array([sign * .205, .89, -.04])
        end = np.array([sign * .356, .45, .095])
        target_start = fit.BONES[side + 'Arm'] + [0, .04, 0]
        target_end = fit.BONES[side + 'Hand'] + [0, .025, 0]
        axis, target_axis = io.unit(end - start), io.unit(target_end - target_start)
        radial = io.unit(np.cross(axis, [0, 0, 1]))
        target_radial = io.unit(np.cross(target_axis, [0, 0, 1]))
        depth = np.cross(axis, radial)
        target_depth = np.cross(target_axis, target_radial)
        local = original - start
        along = (local @ axis) / np.linalg.norm(end - start)
        target = target_start + along[:, None] * (target_end - target_start)
        radial_scale = np.interp(along, [0, .5, 1], [1.4, 1.2, 1.12])
        target += (local @ radial)[:, None] * target_radial * radial_scale[:, None]
        target += (local @ depth)[:, None] * target_depth * radial_scale[:, None]
        amount = sleeve * (original[:, 0] * sign > 0)
        points += (target - points) * amount[:, None]
    return points, sleeve


def fit_lower_robe(points, garment):
    body, faces, weights = fit.body_surface(('torso', 'legs'))
    pose_path = OUTPUT / 'idle-fitting-poses.json'
    animation = json.loads((ROOT / 'doc/assets/modular-priest-tripo-top-animation-v1.json').read_text())
    inputs = [source for source in animation['sources'] if not source['path'].startswith(str(OUTPUT.relative_to(ROOT)))]
    for source in inputs:
        assert fit.digest(ROOT / source['path']) == source['sha256']
    poses = [pose for pose in json.loads(pose_path.read_text()) if pose['clip'] == 'idle1']
    assert len(poses) == 13
    homogeneous = np.column_stack([body, np.ones(len(body))])
    surfaces = [body[faces]]
    for pose in poses:
        matrices = np.asarray(pose['matrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
        relative = np.linalg.inv(matrices[fit.NAMES.index('Hips')]) @ matrices
        transformed = np.einsum('bij,vj->vbi', relative, homogeneous)
        posed = np.einsum('vb,vbi->vi', weights, transformed)[:, :3]
        surfaces.append(posed[faces])
    triangles = np.concatenate(surfaces)
    heights = np.linspace(.62, 1.12, 51)
    angles = np.linspace(-np.pi, np.pi, 129)
    directions = np.column_stack([np.cos(angles), np.sin(angles)])
    center = np.array([0, -.035])
    profiles = []
    for height in heights:
        crossing = (triangles[:, :, 1].min(1) < height) & (triangles[:, :, 1].max(1) > height)
        section = triangles[crossing]
        hits = []
        for a, b in [(0, 1), (1, 2), (2, 0)]:
            start, end = section[:, a], section[:, b]
            valid = (start[:, 1] - height) * (end[:, 1] - height) < 0
            start, end = start[valid], end[valid]
            t = (height - start[:, 1]) / (end[:, 1] - start[:, 1])
            hits.extend(start + (end - start) * t[:, None])
        hull = ConvexHull(np.asarray(hits)[:, [0, 2]] - center)
        denominator = hull.equations[:, :2] @ directions.T
        distances = np.divide(-hull.equations[:, 2, None], denominator,
                              out=np.full_like(denominator, np.inf), where=denominator > 1e-8)
        profiles.append(distances.min(0))
    profiles = np.asarray(profiles)
    indices = np.flatnonzero(garment & (points[:, 1] < 1.12))
    radial = points[indices][:, [0, 2]] - center
    length = np.linalg.norm(radial, axis=1)
    theta = np.arctan2(radial[:, 1], radial[:, 0])
    target = []
    for i, angle in zip(indices, theta):
        radii = np.array([np.interp(angle, angles, row) for row in profiles])
        target.append(np.interp(points[i, 1], heights, radii) + .035)
    amount = 1 - io.smoothstep((points[indices, 1] - 1.08) / .04)
    scale = 1 + (np.maximum(np.asarray(target) / np.maximum(length, 1e-8), 1) - 1) * amount
    original = points[indices].copy()
    points[np.ix_(indices, [0, 2])] = center + radial * scale[:, None]
    displacement = np.linalg.norm(points[indices] - original, axis=1)
    return dict(method='Outward-only fit to torso/thigh sections in rest and sampled idle poses relative to Hips; preserve the front and rear slits',
                idle_samples=len(poses), pose_source=str(pose_path.relative_to(ROOT)),
                pose_sha256=fit.digest(pose_path), pose_inputs=inputs,
                section_margin_m=.035, section_height_range_m=[.62, 1.12],
                changed_vertices=int((displacement > 1e-7).sum()),
                maximum_displacement_m=float(displacement.max()), body_hidden=False)


def main():
    source = OUTPUT / 'source.glb'
    base = ROOT / 'assets/modular_human_male_01/parts/fitted/base.glb'
    assert fit.digest(source) == '63fb9fbe3d1f811bb223eff2fd0a6a7a56e0bb06efbb59461c6e44bd2c2f5859'
    assert fit.digest(base) == '0e629865af6c3feac3a4444e0cb2d5f2d3861bf2350d643cbff0f9858b83535a'
    doc, raw = fit.read_glb(source)
    assert len(doc['meshes']) == 1 and len(doc['meshes'][0]['primitives']) == 1
    prim = doc['meshes'][0]['primitives'][0]
    attrs = prim['attributes']
    original = io.accessor(doc, raw, attrs['POSITION'])
    faces = io.accessor(doc, raw, prim['indices']).reshape(-1, 3)
    uv = io.accessor(doc, raw, attrs['TEXCOORD_0'])
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    _, first, inverse = np.unique(np.round(original, 5), axis=0, return_index=True, return_inverse=True)
    welded = original[first].astype(float)
    points, sleeve = align(welded)
    aligned = points.copy()
    welded_faces = inverse[faces]
    edges = np.unique(np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]], welded_faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(points), len(points)))
    laplacian = sparse.diags(np.asarray(adjacency.sum(1)).ravel()) - adjacency
    smooth = factorized((sparse.eye(len(points)) + .6 * laplacian).tocsc())
    _, components = connected_components(adjacency, directed=False)
    garment = components == np.bincount(components).argmax()
    surface = fit.body_surface(('torso', 'neck', 'upper_arms', 'forearms'))
    active = garment & ((points[:, 1] > 1.10) | (sleeve > .5))
    iterations = []
    for step in range(24):
        nearest, normals, _ = fit.nearest_surface(points, surface, candidates=96)
        signed = np.sum((points - nearest) * normals, axis=1)
        correction = normals * (np.clip(.008 - signed, 0, .012) * active)[:, None]
        correction = np.column_stack([smooth(correction[:, i]) for i in range(3)])
        points += correction
        iterations.append(dict(step=step, minimum_active_signed_distance_m=float(signed[active].min()),
                               maximum_correction_m=float(np.linalg.norm(correction, axis=1).max())))
    lower_clearance = fit_lower_robe(points, garment & (welded[:, 1] < .54))
    for component in np.unique(components[~garment]):
        mask = components == component
        center = aligned[mask].mean(0)
        nearest = np.argsort(np.linalg.norm(aligned[garment] - center, axis=1))[:12]
        points[mask] += (points[garment] - aligned[garment])[nearest].mean(0)
    _, _, transferred = fit.nearest_surface(points, surface, candidates=96)
    dense = np.zeros_like(transferred)
    for side, sign in [('Left', 1), ('Right', -1)]:
        indices = [fit.NAMES.index(side + name) for name in ['Arm', 'ForeArm', 'Hand']]
        weights = transferred[:, indices]
        weights /= np.maximum(weights.sum(1, keepdims=True), 1e-12)
        amount = sleeve * (welded[:, 0] * sign > 0)
        dense[:, indices] = weights * amount[:, None]
    spine = ['Hips', 'Spine', 'Spine1', 'Spine2', 'Neck']
    levels = np.array([1.16, 1.25, 1.36, 1.48, 1.65])
    lower = np.clip(np.searchsorted(levels, points[:, 1]) - 1, 0, len(spine) - 2)
    blend = io.smoothstep((points[:, 1] - levels[lower]) / (levels[lower + 1] - levels[lower]))
    trunk = 1 - dense.sum(1)
    dense[np.arange(len(points)), [fit.NAMES.index(spine[i]) for i in lower]] = trunk * (1 - blend)
    dense[np.arange(len(points)), [fit.NAMES.index(spine[i + 1]) for i in lower]] = trunk * blend
    edge_weights = 1 / np.maximum(np.linalg.norm(points[rows] - points[cols], axis=1), .001) ** 2
    weighted = sparse.csr_matrix((edge_weights, (rows, cols)), shape=adjacency.shape)
    weighted_laplacian = sparse.diags(np.asarray(weighted.sum(1)).ravel()) - weighted
    fidelity = 1 + 60 * sleeve
    diffuse = factorized((sparse.diags(fidelity) + .025 * weighted_laplacian).tocsc())
    dense = np.maximum(np.column_stack([diffuse(dense[:, i] * fidelity) for i in range(len(fit.NAMES))]), 0)
    for side, sign in [('Left', 1), ('Right', -1)]:
        indices = [fit.NAMES.index(side + name) for name in ['Arm', 'ForeArm', 'Hand']]
        dense[:, indices] *= io.smoothstep(points[:, 0] * sign / .10)[:, None]
    dense /= dense.sum(1, keepdims=True)
    ornaments = []
    for component in np.unique(components[~garment]):
        mask = components == component
        bone = 'Hips' if welded[mask, 1].mean() < .65 else 'Spine2'
        dense[mask] = 0
        dense[mask, fit.NAMES.index(bone)] = 1
        ornaments.append(dict(component=int(component), vertices=int(mask.sum()), bone=bone))
    hem = (welded[:, 1] < .40) & garment
    dense[hem] = 0
    dense[hem, fit.NAMES.index('Hips')] = 1
    joints, weights = compact_weights(dense)
    positions = points[inverse]
    binary = bytearray(raw)
    attrs.update(io.add_skin_attributes(doc, binary, positions, io.smooth_normals(positions, faces), joints[inverse], weights[inverse]))
    attrs.pop('TANGENT', None)
    doc['meshes'][0]['name'] = 'top_priest_tripo'
    fit.with_rig(doc, binary, 'top_priest', 'tripo_v1')
    robe = dict(waist_height_m=1.08, hem_height_m=.64, center_z_m=-.035,
                waist_radii_m=[.18, .14],
                vertices=np.flatnonzero((garment & (sleeve < .1) & (points[:, 1] < 1.08))[inverse]).tolist())
    for node in doc['nodes']:
        if 'mesh' in node:
            node['extras']['robe_physics'] = robe
            node['extras']['cloth_status'] = 'Lightweight runtime hem: eight angular springs and auxiliary GPU bones; exact cloth collisions not implemented'
    binary = io.compact(doc, binary)
    target = OUTPUT / 'top_priest.glb'
    fit.write_glb(target, doc, binary)
    exported_attrs = doc['meshes'][0]['primitives'][0]['attributes']
    assert np.array_equal(uv, io.accessor(doc, binary, exported_attrs['TEXCOORD_0']))
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    assert np.isfinite(positions).all()
    report = dict(date='2026-10-08', status='Fitting candidate with lightweight eight-bone runtime hem; extreme-pose clearance and mixed equipment acceptance pending',
                  source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
                  base=dict(path=str(base.relative_to(ROOT)), sha256=fit.digest(base)),
                  rig_id='human_male_01_mixamo_candidate_v2', source_triangles=len(faces),
                  method='Independent sleeve-axis alignment, smooth body clearance, axial torso weights and rigid detached ornaments',
                  alignment=dict(trunk_scale=[1.14, 1.16, 1.12], offset=[0, .48, -.05], hem_height_m=.64,
                                 sleeve_source_start=[.205, .89, -.04], sleeve_source_end=[.356, .45, .095],
                                 sleeves_target='Existing Arm +40mm to Hand +25mm; mirrored for right',
                                 sleeve_radial_scale=[1.4, 1.2, 1.12]),
                  preserved_source_indices_uv_and_embedded_texture=True, clearance_target_m=.008,
                  weight_diffusion=dict(strength=.025, sleeve_fidelity=60, opposite_arm_influences_removed=True),
                  correction_iterations=iterations, rigid_ornaments=ornaments,
                  lower_robe_clearance=lower_clearance,
                  cloth=dict(status='Eight runtime auxiliary bones with angular springs; approximate thigh clearance',
                             simulation_hz=30, simulated_controls=8, added_render_vertices=0,
                             exported_hem_attachment='Hips 100%; runtime remaps garment vertices only',
                             runtime_config=robe,
                             vertices=np.flatnonzero(hem[inverse]).tolist(),
                             warning='GPU spring hem requires separate runtime motion review; source skinning validators do not run the spring rig'),
                  validation=fit.validate(target))
    report_path = ROOT / 'doc/assets/modular-priest-tripo-top-fitting-v1.json'
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation'], indent=2))


if __name__ == '__main__':
    main()
