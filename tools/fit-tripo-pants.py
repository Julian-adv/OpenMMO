"""Fit the preserved Tripo trousers to the current modular male and its rig."""
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy import sparse
from scipy.sparse.csgraph import connected_components
from scipy.sparse.linalg import factorized

from lib.glb import view_bytes
from outfits.rogue_layers import compact_weights, wrist_section

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/rogue_tripo_pants_v1'
spec = importlib.util.spec_from_file_location('rogue_fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io


def initial_fit(points):
    result = points.astype(float).copy()
    result[:, 1] = np.interp(points[:, 1], [0, .40, .70, .99951171875], [.181, .553, .862, 1.155])
    waist = io.smoothstep((result[:, 1] - .90) / .18)
    result[:, 0] *= .85 + .23 * waist
    result[:, 2] = points[:, 2] * (.95 + .20 * waist) - .027
    return result


def fit_cuffs(points, faces):
    result = points.copy()
    report = []
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        center, axis, basis, _, _ = fit.ankle_interface(side)
        triangles = points[faces]
        triangles = triangles[triangles.mean(1)[:, 0] * sign > 0]
        heights = np.linspace(.184, .43, 20)
        sections = [io.section_bounds(triangles, np.array([0, height, 0]),
            np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]])) for height in heights]
        centers = np.array([(low + high) / 2 for low, high in sections])
        radii = np.array([(high - low) / 2 for low, high in sections])
        mask = (points[:, 0] * sign > 0) & (points[:, 1] < .43)
        local = points[mask]
        origin = np.column_stack([np.interp(local[:, 1], heights, centers[:, i]) for i in range(2)])
        extent = np.column_stack([np.interp(local[:, 1], heights, radii[:, i]) for i in range(2)])
        cross = (local[:, [0, 2]] - origin) / extent
        angle = np.arctan2(cross[:, 1], cross[:, 0])
        height = local[:, 1] - center[1] - .004
        radius = fit.skin_radius(side, height, angle) + .004
        target = center + height[:, None] * axis
        target += radius[:, None] * (np.cos(angle)[:, None] * basis[0] + np.sin(angle)[:, None] * basis[1])
        amount = 1 - io.smoothstep((local[:, 1] - .26) / .17)
        result[mask] = local * (1 - amount[:, None]) + target * amount[:, None]
        report.append(dict(side=side, skin_radial_clearance_m=.004, transition_interval_y_m=[.26, .43],
            minimum_hem_y_m=float(result[mask, 1].min()), method='Current v1 ankle contours; no repeated calf slimming'))
    return result, report


def ankle_references():
    result = []
    for side in ['Left', 'Right']:
        center, axis, basis, _, weights = fit.ankle_interface(side)
        result.append(dict(side=side, interface='shoe_ankle_' + side,
            center=center.tolist(), axis=axis.tolist(), basis=basis.tolist(),
            weights={name: float(weight) for name, weight in zip(fit.NAMES, weights) if weight > 0},
            sample_heights_m=np.linspace(-.030, 0, 7).tolist()))
    return result


def surface_clearance(samples):
    correction = np.zeros_like(samples)
    signed = np.zeros(len(samples))
    for sign in [1, -1]:
        indices = np.where(samples[:, 0] * sign >= 0)[0]
        surface = fit.body_surface(('legs', 'ankles', 'torso'), sign)
        for start in range(0, len(indices), 256):
            selected = indices[start:start + 256]
            nearest, normals, _ = fit.nearest_surface(samples[selected], surface, candidates=96)
            signed[selected] = np.sum((samples[selected] - nearest) * normals, axis=1)
            target = .003 + .005 * io.smoothstep((samples[selected, 1] - .27) / .12)
            correction[selected] = normals * np.clip(target - signed[selected], 0, .020)[:, None]
    return correction, signed


def clear_faces(points, faces, main_component, smoother):
    cloth_faces = faces[np.all(main_component[faces], axis=1)]
    sample_weights = np.array([[1 / 3, 1 / 3, 1 / 3], [.5, .5, 0], [0, .5, .5], [.5, 0, .5]])
    iterations = []
    movable = io.smoothstep((points[:, 1] - .30) / .06)
    for step in range(20):
        samples = np.einsum('sk,fkj->fsj', sample_weights, points[cloth_faces]).reshape(-1, 3)
        corrections, signed = surface_clearance(samples)
        corrections = corrections.reshape(-1, 4, 3)
        active = np.linalg.norm(corrections, axis=2) > 1e-6
        accumulated = np.zeros_like(points)
        total = np.zeros(len(points))
        for sample, weights in enumerate(sample_weights):
            for corner, weight in enumerate(weights):
                np.add.at(accumulated, cloth_faces[:, corner], corrections[:, sample] * weight)
                np.add.at(total, cloth_faces[:, corner], active[:, sample] * weight)
        delta = accumulated / np.maximum(total, 1)[:, None]
        delta = smoother(delta) * movable[:, None]
        points += delta
        reviewed = samples[:, 1] > .36
        iterations.append(dict(step=step, minimum_sample_signed_distance_m=float(signed[reviewed].min()),
            maximum_correction_m=float(np.linalg.norm(delta, axis=1).max())))
    return points, dict(method='Cloth triangle centers and three edge midpoints against current body surfaces; boot interface below .30m remains fixed',
        reviewed_sample_minimum_y_m=.36,
        samples_per_iteration=len(cloth_faces) * 4, iterations=iterations)


def fit_waist(points, faces, main_component):
    result = points.copy()
    surface = fit.body_surface(('torso', 'legs'))
    body_triangles = surface[0][surface[1]]
    triangles = points[faces[np.all(main_component[faces], axis=1)]]
    heights = np.linspace(1.0, 1.13, 25)
    theta = np.arange(96) * 2 * np.pi / 96
    radial = np.column_stack([np.cos(theta), np.sin(theta)])
    source_radius, body_radius, centers = [], [], []
    for height in heights:
        mid = np.array([0, np.interp(height, [fit.BONES['Hips'][1], fit.BONES['Spine'][1]],
            [fit.BONES['Hips'][2], fit.BONES['Spine'][2]])])
        args = (np.array([0, height, 0]), np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]]), radial)
        _, radius = wrist_section(triangles, *args, mid=mid, outermost=False)
        source_radius.append(radius)
        _, radius = wrist_section(body_triangles, *args, mid=mid, outermost=False)
        body_radius.append(radius)
        centers.append(mid)
    source_radius, body_radius = np.array(source_radius), np.array(body_radius)
    mask = points[:, 1] > .99
    local = points[mask]
    center = np.column_stack([np.interp(local[:, 1], heights, np.array(centers)[:, i]) for i in range(2)])
    offset = local[:, [0, 2]] - center
    angle = np.arctan2(offset[:, 1], offset[:, 0])
    radius = np.linalg.norm(offset, axis=1)
    source = np.array([np.interp(angle, theta, row, period=2 * np.pi) for row in source_radius])
    body = np.array([np.interp(angle, theta, row, period=2 * np.pi) for row in body_radius])
    source = np.array([np.interp(y, heights, source[:, i]) for i, y in enumerate(local[:, 1])])
    body = np.array([np.interp(y, heights, body[:, i]) for i, y in enumerate(local[:, 1])])
    target = body + .012 + (radius - source)
    amount = io.smoothstep((local[:, 1] - .99) / .09)
    fitted = center + offset * (1 + (target / radius - 1) * amount)[:, None]
    result[np.ix_(mask, [0, 2])] = fitted
    return result, dict(method='Actual trouser and body radial waist sections; accessories retain their offset from the cloth',
        section_y_m=[float(heights[0]), float(heights[-1])], cloth_clearance_m=.012,
        maximum_displacement_m=float(np.linalg.norm(result - points, axis=1).max()))


def skin_weights(points, adjacency, main_component):
    surface = fit.body_surface(('legs', 'ankles', 'torso'))
    _, _, dense = fit.nearest_surface(points, surface, candidates=96)
    allowed = {'Hips', 'Spine', 'LeftUpLeg', 'LeftLeg', 'LeftFoot', 'RightUpLeg', 'RightLeg', 'RightFoot'}
    dense[:, [i for i, name in enumerate(fit.NAMES) if name not in allowed]] = 0
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        side_blend = io.smoothstep((points[:, 0] * sign + .01) / .06)
        for bone in ['UpLeg', 'Leg', 'Foot']:
            dense[:, fit.NAMES.index(side + bone)] *= side_blend
    dense /= dense.sum(1, keepdims=True)
    width = .065 + .035 * io.smoothstep((points[:, 1] - .82) / .14)
    left = io.smoothstep((points[:, 0] + width) / (2 * width))
    hip = io.smoothstep((points[:, 1] - .82) / .16)
    pelvis = np.zeros_like(dense)
    pelvis[:, fit.NAMES.index('Hips')] = hip
    pelvis[:, fit.NAMES.index('LeftUpLeg')] = (1 - hip) * left
    pelvis[:, fit.NAMES.index('RightUpLeg')] = (1 - hip) * (1 - left)
    blend = io.smoothstep((points[:, 1] - .76) / .06)
    blend *= 1 - io.smoothstep((abs(points[:, 0]) - .06) / .13)
    dense = dense * (1 - blend[:, None]) + pelvis * blend[:, None]
    rows, cols = adjacency.nonzero()
    edge_weights = 1 / np.maximum(np.linalg.norm(points[rows] - points[cols], axis=1), .002) ** 2
    weighted = sparse.csr_matrix((edge_weights, (rows, cols)), shape=adjacency.shape)
    laplacian = sparse.diags(np.asarray(weighted.sum(1)).ravel()) - weighted
    fidelity = np.full(len(points), 10.)
    smoother = factorized((sparse.diags(fidelity) + .003 * laplacian).tocsc())
    dense = np.maximum(smoother(dense * fidelity[:, None]), 0)
    hip = io.smoothstep((points[:, 1] - .91) / .10)
    hip[~main_component] = 1
    dense *= 1 - hip[:, None]
    dense[:, fit.NAMES.index('Hips')] += hip
    dense = np.maximum(dense - .007, 0)
    dense /= dense.sum(1, keepdims=True)
    joints, weights = compact_weights(dense)
    joints, weights = fit.ankle_weights(points, joints, weights)
    return joints, weights


def main():
    sources = json.loads((ROOT / 'doc/assets/modular-rogue-tripo-pants-sources.json').read_text())
    for source in [sources['source'], sources['base'], sources['interfaces']]:
        assert fit.digest(ROOT / source['path']) == source['sha256']
    doc, raw = fit.read_glb(OUTPUT / 'source.glb')
    assert len(doc['meshes']) == 1 and len(doc['meshes'][0]['primitives']) == 1
    primitive = doc['meshes'][0]['primitives'][0]
    original = io.accessor(doc, raw, primitive['attributes']['POSITION'])
    original_uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    original_images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    source_triangles = len(faces)
    triangles = original[faces]
    normals = io.unit(np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]))
    waist_center = (original[:, 1] > .97) & (abs(original[:, 0]) < .01) & (abs(original[:, 2]) < .04)
    waist_caps = np.any(waist_center[faces], axis=1)
    ankle_caps = (triangles[:, :, 1].max(1) < .03) & (normals[:, 1] < -.8)
    assert waist_caps.sum() == 20 and ankle_caps.sum() == 24
    faces = faces[~(waist_caps | ankle_caps)]
    used, remapped = np.unique(faces, return_inverse=True)
    original, original_uv = original[used], original_uv[used]
    faces = remapped.reshape(-1, 3)
    points, inverse = np.unique(original, axis=0, return_inverse=True)
    welded_faces = inverse[faces]
    edges = np.unique(np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]], welded_faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(points), len(points)))
    _, component = connected_components(adjacency)
    main_component = component == np.bincount(component).argmax()
    points = initial_fit(points)
    points, cuffs = fit_cuffs(points, welded_faces)
    points, waist = fit_waist(points, welded_faces, main_component)
    laplacian = sparse.diags(np.asarray(adjacency.sum(1)).ravel()) - adjacency
    smooth = factorized((sparse.eye(len(points)) + .7 * laplacian).tocsc())
    iterations = []
    for step in range(30):
        correction = np.zeros_like(points)
        correction[main_component], clearances = surface_clearance(points[main_component])
        correction = smooth(correction)
        points += correction
        iterations.append(dict(step=step, minimum_signed_distance_m=float(min(clearances)),
            maximum_correction_m=float(np.linalg.norm(correction, axis=1).max())))
    points, face_clearance = clear_faces(points, welded_faces, main_component, smooth)
    joints, weights = skin_weights(points, adjacency, main_component)
    binary = bytearray(raw)
    positions = points[inverse]
    normals = io.smooth_normals(positions, faces)
    primitive['attributes'].update(io.add_skin_attributes(doc, binary, positions,
        normals, joints[inverse], weights[inverse]))
    primitive['attributes']['TEXCOORD_0'] = io.add_accessor(doc, binary, original_uv, 'VEC2')
    primitive['indices'] = io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125)
    if 'TANGENT' in primitive['attributes']:
        primitive['attributes']['TANGENT'] = io.add_accessor(doc, binary,
            io.tangents(positions, normals, original_uv, faces), 'VEC4')
    doc['meshes'][0]['name'] = 'pants_rogue_tripo'
    fit.with_rig(doc, binary, 'pants_rogue', 'tripo_pants_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'pants_rogue.glb'
    fit.write_glb(target, doc, binary)
    assert np.array_equal(original_uv, io.accessor(doc, binary, primitive['attributes']['TEXCOORD_0']))
    assert original_images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    report = dict(status='Fitted and rigged candidate; animation and visual review required after rebuilding',
        source=sources['source'], base=sources['base'], interfaces=sources['interfaces'],
        rig_id=sources['rig_id'], source_triangles=source_triangles, preserved_retained_vertex_uv_and_embedded_texture=True,
        topology_changes=dict(removed_solid_waist_cap_triangles=int(waist_caps.sum()),
            removed_solid_ankle_cap_triangles=int(ankle_caps.sum()), retained_source_vertices=used.tolist(),
            method='Remove source cap faces only; retain the waistband rim and ankle hem shapes'),
        method='Anatomical alignment, actual waist sections, current ankle contours, smooth skin clearance and surface-transferred leg weights',
        cuffs=cuffs, waist=waist, clearance_iterations=iterations, validation=fit.validate(target),
        ankle_connections=ankle_references(),
        face_clearance=face_clearance,
        rigid_hip_accessories=int(np.sum(~main_component)),
        remaining_review=['Dynamic garment/body intersections', 'Waist overlap with selected vest', 'Ankle overlap with boots', 'Mixed equipment and barefoot variants'])
    (ROOT / 'doc/assets/modular-rogue-tripo-pants-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation'], indent=2))


if __name__ == '__main__':
    main()
