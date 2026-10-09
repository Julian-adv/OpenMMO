"""Fit and skin the preserved ranger top to the canonical modular male."""
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


def section_bounds(triangles, height):
    crossing = (triangles[:, :, 1].min(1) < height) & (triangles[:, :, 1].max(1) > height)
    triangles = triangles[crossing]
    hits = []
    for a, b in [(0, 1), (1, 2), (2, 0)]:
        start, end = triangles[:, a], triangles[:, b]
        valid = (start[:, 1] - height) * (end[:, 1] - height) < 0
        start, end = start[valid], end[valid]
        t = (height - start[:, 1]) / (end[:, 1] - start[:, 1])
        hits.extend(start + (end - start) * t[:, None])
    hits = np.asarray(hits)
    assert len(hits) > 0
    return hits.min(0), hits.max(0)


def tighten(points, faces):
    original = points.copy()
    heights = np.linspace(1.10, 1.48, 39)
    body = fit.body_surface(('torso', 'legs'))
    cloth = points[faces]
    cloth = cloth[(abs(cloth[:, :, 0]) < .26).all(1)]
    body_bounds = np.array([section_bounds(body[0][body[1]], height) for height in heights])
    cloth_bounds = np.array([section_bounds(cloth, height) for height in heights])
    trunk_edge = np.interp(points[:, 1], [1.05, 1.24, 1.40, 1.50], [.28, .22, .20, .17])
    amount = 1 - io.smoothstep((abs(points[:, 0]) - trunk_edge) / .07)
    amount *= 1 - io.smoothstep((points[:, 1] - 1.47) / .06)
    width_scale = np.interp(points[:, 1], [1.05, 1.12, 1.22, 1.35, 1.48], [.74, .80, .86, .90, .96])
    points[:, 0] *= 1 - (1 - width_scale) * amount
    low = np.interp(points[:, 1], heights, cloth_bounds[:, 0, 2])
    high = np.interp(points[:, 1], heights, cloth_bounds[:, 1, 2])
    target_low = np.interp(points[:, 1], heights, body_bounds[:, 0, 2]) - .007
    target_high = np.interp(points[:, 1], heights, body_bounds[:, 1, 2]) + .007
    fraction = (points[:, 2] - low) / (high - low)
    target = target_low + fraction * (target_high - target_low)
    points[:, 2] += (target - points[:, 2]) * amount * .9
    sleeve_amount = io.smoothstep((abs(original[:, 0]) - .24) / .07)
    sleeve_amount *= 1 - io.smoothstep((original[:, 1] - 1.46) / .07)
    for side, sign in [('Left', 1), ('Right', -1)]:
        start, end = fit.BONES[side + 'Arm'], fit.BONES[side + 'ForeArm']
        axis = io.unit(end - start)
        center = start + ((original - start) @ axis)[:, None] * axis
        radial = original - center
        points -= radial * (.12 * sleeve_amount * (original[:, 0] * sign > 0))[:, None]
    displacement = np.linalg.norm(points - original, axis=1)
    return points, dict(method='Reduce waist and hem flare, match torso depth to body sections and reduce sleeve radial volume',
                        torso_width_scales=width_scale[[np.argmin(abs(original[:, 1] - h)) for h in [1.05, 1.12, 1.22, 1.35, 1.48]]].tolist(),
                        torso_depth_margin_m=.007, sleeve_radial_reduction=.12,
                        maximum_vertex_displacement_m=float(displacement.max()),
                        changed_vertices=int((displacement > 1e-7).sum()))


def main(source, output, report_path):
    base = ROOT / 'assets/modular_human_male_01/fitted/base.glb'
    assert fit.digest(source) == 'f4988d1e5d44ffbd5a8af3ec9cb9f1f601de416611ebb37a928b61a940a74ce5'
    assert fit.digest(base) == '0e629865af6c3feac3a4444e0cb2d5f2d3861bf2350d643cbff0f9858b83535a'
    doc, raw = fit.read_glb(source)
    assert len(doc['meshes']) == 1 and len(doc['meshes'][0]['primitives']) == 1
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    original = io.accessor(doc, raw, attrs['POSITION'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    uv = io.accessor(doc, raw, attrs['TEXCOORD_0'])
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    _, first, inverse = np.unique(np.round(original, 5), axis=0, return_index=True, return_inverse=True)
    points = original[first].astype(float)
    welded_faces = inverse[faces]
    x_scale = 1.12 - .34 * io.smoothstep((abs(points[:, 0]) - .20) / .15)
    points[:, 0] *= x_scale
    points[:, 1] = points[:, 1] * .80 + 1.05
    points[:, 2] = points[:, 2] * .90 - .025
    edges = np.unique(np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]], welded_faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(points), len(points)))
    laplacian = sparse.diags(np.asarray(adjacency.sum(1)).ravel()) - adjacency
    smooth = factorized((sparse.eye(len(points)) + 1.5 * laplacian).tocsc())
    surface = fit.body_surface(('torso', 'neck', 'upper_arms', 'forearms'))
    iterations = []
    for step in range(20):
        nearest, normals, _ = fit.nearest_surface(points, surface, candidates=128)
        signed = np.sum((points - nearest) * normals, axis=1)
        correction = normals * np.clip(.010 - signed, 0, .018)[:, None]
        correction = np.column_stack([smooth(correction[:, i]) for i in range(3)])
        points += correction
        iterations.append(dict(step=step, minimum_signed_distance_m=float(signed.min()),
                               maximum_correction_m=float(np.linalg.norm(correction, axis=1).max())))
    points, tightening = tighten(points, welded_faces)
    for step in range(16):
        nearest, normals, _ = fit.nearest_surface(points, surface, candidates=128)
        signed = np.sum((points - nearest) * normals, axis=1)
        correction = normals * np.clip(.006 - signed, 0, .010)[:, None]
        correction = np.column_stack([smooth(correction[:, i]) for i in range(3)])
        points += correction
    pants_path = ROOT / 'assets/modular_human_male_01/fitted/pants_cloth.glb'
    pants_doc, pants_binary = fit.read_glb(pants_path)
    pants_points, pants_faces = [], []
    for mesh in pants_doc['meshes']:
        for prim in mesh['primitives']:
            p = io.accessor(pants_doc, pants_binary, prim['attributes']['POSITION'])
            f = io.accessor(pants_doc, pants_binary, prim['indices']).reshape(-1, 3)
            pants_faces.extend(f + len(pants_points))
            pants_points.extend(p)
    pants_points, pants_faces = np.asarray(pants_points), np.asarray(pants_faces)
    trunk = fit.body_surface(('torso', 'legs'))
    heights = np.linspace(1.05, 1.16, 23)
    bounds = []
    for height in heights:
        low, high = section_bounds(trunk[0][trunk[1]], height)
        if height < pants_points[:, 1].max():
            pants_low, pants_high = section_bounds(pants_points[pants_faces], height)
            low, high = np.minimum(low, pants_low), np.maximum(high, pants_high)
        bounds.append([low, high])
    bounds = np.asarray(bounds)
    center = np.interp(points[:, 1], heights, bounds[:, :, 2].mean(1))
    x_radius = np.interp(points[:, 1], heights, abs(bounds[:, :, 0]).max(1)) + .008
    z_radius = np.interp(points[:, 1], heights, (bounds[:, 1, 2] - bounds[:, 0, 2]) / 2) + .008
    radial = np.column_stack([points[:, 0], points[:, 2] - center])
    radius = np.linalg.norm(radial / np.column_stack([x_radius, z_radius]), axis=1)
    hem_amount = 1 - io.smoothstep((points[:, 1] - 1.12) / .04)
    scale = 1 + (np.maximum(1 / np.maximum(radius, 1e-6), 1) - 1) * hem_amount
    points[:, 0] *= scale
    points[:, 2] = center + (points[:, 2] - center) * scale
    tightening['context_waist_clearance'] = dict(path=str(pants_path.relative_to(ROOT)),
                                                sha256=fit.digest(pants_path), envelope_margin_m=.008,
                                                scope='Lower hem versus existing cloth pants; other pants unverified')
    _, _, dense = fit.nearest_surface(points, surface, candidates=128)
    permitted = {'Hips', 'Spine', 'Spine1', 'Spine2', 'Neck', 'LeftShoulder', 'LeftArm', 'LeftForeArm',
                 'RightShoulder', 'RightArm', 'RightForeArm'}
    dense[:, [i for i, name in enumerate(fit.NAMES) if name not in permitted]] = 0
    assert (dense.sum(1) > 0).all()
    dense /= dense.sum(1, keepdims=True)
    for side in ['Left', 'Right']:
        shoulder = fit.NAMES.index(side + 'Shoulder')
        dense[:, fit.NAMES.index('Spine2')] += dense[:, shoulder] * .5
        dense[:, fit.NAMES.index(side + 'Arm')] += dense[:, shoulder] * .5
        dense[:, shoulder] = 0
    edge_weights = 1 / np.maximum(np.linalg.norm(points[rows] - points[cols], axis=1), .001) ** 2
    weighted = sparse.csr_matrix((edge_weights, (rows, cols)), shape=adjacency.shape)
    weight_laplacian = sparse.diags(np.asarray(weighted.sum(1)).ravel()) - weighted
    fidelity = 1 + 80 * io.smoothstep((abs(points[:, 0]) - .24) / .10)
    smooth_weights = factorized((sparse.diags(fidelity) + .05 * weight_laplacian).tocsc())
    dense = np.maximum(np.column_stack([smooth_weights(dense[:, i] * fidelity) for i in range(len(fit.NAMES))]), 0)
    local = np.zeros_like(dense)
    for side, sign in [('Left', 1), ('Right', -1)]:
        arm = [fit.NAMES.index(side + name) for name in ['Arm', 'ForeArm']]
        local[:, arm] = dense[:, arm] * io.smoothstep(points[:, 0] * sign / .12)[:, None]
    spine = ['Hips', 'Spine', 'Spine1', 'Spine2', 'Neck']
    levels = np.array([fit.BONES[name][1] for name in spine])
    lower = np.clip(np.searchsorted(levels, points[:, 1]) - 1, 0, len(spine) - 2)
    blend = io.smoothstep((points[:, 1] - levels[lower]) / (levels[lower + 1] - levels[lower]))
    trunk = 1 - local.sum(1)
    local[np.arange(len(points)), [fit.NAMES.index(spine[i]) for i in lower]] = trunk * (1 - blend)
    local[np.arange(len(points)), [fit.NAMES.index(spine[i + 1]) for i in lower]] = trunk * blend
    _, labels = connected_components(adjacency, directed=False)
    main_component = np.bincount(labels).argmax()
    garment = labels == main_component
    tree = cKDTree(points[garment])
    button_groups = []
    for component in np.unique(labels[~garment]):
        button = labels == component
        nearest = tree.query(points[button].mean(0), k=8)[1]
        shared = local[garment][nearest].mean(0)
        local[button] = shared / shared.sum()
        button_groups.append(dict(vertices=int(button.sum()), shared_weights=True))
    dense = local / local.sum(1, keepdims=True)
    joints, weights = compact_weights(dense)
    positions = points[inverse]
    binary = bytearray(raw)
    attrs.update(io.add_skin_attributes(doc, binary, positions, io.smooth_normals(positions, faces), joints[inverse], weights[inverse]))
    attrs.pop('TANGENT', None)
    doc['meshes'][0]['name'] = 'top_ranger_tripo'
    fit.with_rig(doc, binary, 'top_ranger', 'tripo_v2')
    binary = io.compact(doc, binary)
    target = output / 'top_ranger.glb'
    target.parent.mkdir(parents=True, exist_ok=True)
    fit.write_glb(target, doc, binary)
    exported_attrs = doc['meshes'][0]['primitives'][0]['attributes']
    assert np.array_equal(uv, io.accessor(doc, binary, exported_attrs['TEXCOORD_0']))
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    samples = np.concatenate([positions, positions[faces].mean(1)])
    near, normals, _ = fit.nearest_surface(samples, surface, candidates=128)
    signed = np.sum((samples - near) * normals, axis=1)
    report = dict(date='2026-10-05', status='Fitted and rigged candidate; visual, animation and mixed equipment acceptance pending',
                  source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
                  base=dict(path=str(base.relative_to(ROOT)), sha256=fit.digest(base)),
                  rig_id='human_male_01_mixamo_candidate_v2',
                  method='Tighter torso section fit and slimmer sleeves, smooth body clearance and seam-consistent skin transfer',
                  initial_alignment=dict(torso_x_scale=1.12, sleeve_x_scale=.78, transition_source_abs_x=[.20, .35],
                                         y_scale=.80, y_offset=1.05, z_scale=.90, z_offset=-.025),
                  source_triangles=len(faces), preserved_uv_and_embedded_texture=True,
                  clearance_target_m=.006, initial_clearance_target_m=.010, correction_iterations=iterations,
                  tightening=tightening,
                  skin_weight_policy=dict(surface_diffusion=.05, sleeve_fidelity=80, torso='Smooth axial spine blend',
                                          detached_buttons=button_groups, maximum_influences=4),
                  rest_surface_diagnostic=dict(samples=len(samples), minimum_signed_distance_m=float(signed.min()),
                                               negative_samples=int((signed < 0).sum()),
                                               interpretation='Nearest signed distance includes internal collar and cuffs; not an intersection acceptance test'),
                  validation=fit.validate(target))
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: report[key] for key in ['validation', 'rest_surface_diagnostic']}, indent=2))
    return report


if __name__ == '__main__':
    raise SystemExit('Use tools/build-ranger-top.py to build the final asset.')
