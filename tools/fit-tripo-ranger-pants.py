"""Fit ranger trousers to the current male body while preserving source UVs."""
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy import sparse
from scipy.sparse.csgraph import connected_components
from scipy.sparse.linalg import factorized

from lib.glb import view_bytes
from outfits.rogue_layers import wrist_section

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/ranger_tripo_pants_v1'
spec = importlib.util.spec_from_file_location('pants', ROOT / 'tools/fit-tripo-pants.py')
pants = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pants)
fit, io = pants.fit, pants.io
spec = importlib.util.spec_from_file_location('waist_review', ROOT / 'tools/review-ranger-waist.py')
waist_review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(waist_review)


def initial_fit(points):
    result = points.astype(float).copy()
    result[:, 1] = np.interp(points[:, 1], [0, .40, .75, .99951171875], [.181, .553, .870, 1.150])
    waist = io.smoothstep((result[:, 1] - .90) / .18)
    result[:, 0] *= .85 + .23 * waist
    result[:, 2] = points[:, 2] * (.95 + .20 * waist) - .027
    return result


def fit_waist(points, faces, main_component):
    result = points.copy()
    surface = fit.body_surface(('torso', 'legs'))
    body_triangles = surface[0][surface[1]]
    triangles = points[faces[np.all(main_component[faces], axis=1)]]
    heights = np.linspace(1.0, 1.095, 20)
    theta = np.arange(96) * 2 * np.pi / 96
    radial = np.column_stack([np.cos(theta), np.sin(theta)])
    source_radii, body_radii, centers = [], [], []
    for height in heights:
        mid = np.array([0, np.interp(height, [fit.BONES['Hips'][1], fit.BONES['Spine'][1]],
                                   [fit.BONES['Hips'][2], fit.BONES['Spine'][2]])])
        args = (np.array([0, height, 0]), np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]]), radial)
        source_radii.append(wrist_section(triangles, *args, mid=mid, outermost=False)[1])
        body_radii.append(wrist_section(body_triangles, *args, mid=mid, outermost=False)[1])
        centers.append(mid)
    mask = points[:, 1] > .99
    local = points[mask]
    center = np.column_stack([np.interp(local[:, 1], heights, np.array(centers)[:, i]) for i in range(2)])
    offset = local[:, [0, 2]] - center
    angle = np.arctan2(offset[:, 1], offset[:, 0])
    radius = np.linalg.norm(offset, axis=1)
    def sample(profiles):
        angular = np.array([np.interp(angle, theta, row, period=2 * np.pi) for row in profiles])
        return np.array([np.interp(y, heights, angular[:, i]) for i, y in enumerate(local[:, 1])])
    target = sample(body_radii) + .009 + (radius - sample(source_radii))
    amount = io.smoothstep((local[:, 1] - .99) / .09)
    result[np.ix_(mask, [0, 2])] = center + offset * (1 + (target / radius - 1) * amount)[:, None]
    return result, dict(section_y_m=heights.tolist(), cloth_clearance_m=.009,
                        method='Closed sections below sloping open waist; accessory offsets retained',
                        maximum_displacement_m=float(np.linalg.norm(result - points, axis=1).max()))


def tuck_belt_under_vest(points, faces, component, main_component):
    top_path = 'assets/modular_human_male_01/parts/ranger_tripo_top_v4/top_ranger.glb'
    top = waist_review.load(top_path)
    triangles = np.concatenate([vertices[faces] for vertices, faces, _, _ in top])
    heights = np.arange(1.065, 1.156, .002)
    angles = np.arange(120) * 2 * np.pi / 120
    profiles = np.array([waist_review.radii(triangles, y) for y in heights])
    result = points.copy()
    center = np.array([0, -.005])
    waist_faces = faces[points[faces, 1].min(1) >= 1.0]
    sample_weights = np.array([[1, 0, 0], [0, 1, 0], [0, 0, 1],
                               [1 / 3, 1 / 3, 1 / 3], [.5, .5, 0], [.5, 0, .5], [0, .5, .5]])
    iterations = []
    accessories = [np.where(component == group)[0] for group in np.unique(component[~main_component])]
    for step in range(20):
        samples = np.einsum('sk,fkj->fsj', sample_weights, result[waist_faces]).reshape(-1, 3)
        offset = samples[:, [0, 2]] - center
        radius = np.linalg.norm(offset, axis=1)
        theta = np.arctan2(offset[:, 1], offset[:, 0])
        angular = np.array([np.interp(theta, angles, row, period=2 * np.pi) for row in profiles])
        limit = np.array([np.interp(y, heights, angular[:, i]) for i, y in enumerate(samples[:, 1])]) - .009
        covered = (samples[:, 1] >= heights[0]) & (samples[:, 1] <= heights[-1]) & np.isfinite(limit)
        correction = np.where(covered, np.maximum(radius - limit, 0), 0)
        delta = np.zeros_like(samples)
        delta[:, [0, 2]] = -offset * (correction / radius)[:, None]
        delta = delta.reshape(-1, len(sample_weights), 3)
        accumulated, total = np.zeros_like(result), np.zeros(len(result))
        for sample, weights in enumerate(sample_weights):
            active = np.linalg.norm(delta[:, sample], axis=1) > 1e-6
            for corner, weight in enumerate(weights):
                np.add.at(accumulated, waist_faces[:, corner], delta[:, sample] * weight)
                np.add.at(total, waist_faces[:, corner], active * weight)
        change = accumulated / np.maximum(total, 1)[:, None]
        for indices in accessories:
            inward = center - result[indices][:, [0, 2]].mean(0)
            inward /= np.linalg.norm(inward)
            amount = max(0, float((change[indices][:, [0, 2]] @ inward).max()))
            change[indices] = 0
            change[np.ix_(indices, [0, 2])] = inward * amount
        result += change
        iterations.append(dict(step=step, maximum_correction_m=float(correction.max())))
    return result, dict(top_path=top_path, top_sha256=fit.digest(ROOT / top_path),
                        method='Constrain waist vertices, triangle centers and edge midpoints beneath ranger vest sections; translate detached accessories rigidly and preserve open slit visibility',
                        target_clearance_m=.009, section_y_m=[float(heights[0]), float(heights[-1])],
                        iterations=iterations,
                        corrected_vertices=int((np.linalg.norm(result - points, axis=1) > 1e-6).sum()),
                        maximum_displacement_m=float(np.linalg.norm(result - points, axis=1).max()))


def main():
    sources = json.loads((ROOT / 'doc/assets/modular-ranger-tripo-pants-sources.json').read_text())
    for source in [sources['source'], sources['base'], sources['interfaces']]:
        assert fit.digest(ROOT / source['path']) == source['sha256']
    doc, raw = fit.read_glb(OUTPUT / 'source.glb')
    assert len(doc['meshes']) == 1 and len(doc['meshes'][0]['primitives']) == 1
    primitive = doc['meshes'][0]['primitives'][0]
    original = io.accessor(doc, raw, primitive['attributes']['POSITION'])
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    points, inverse = np.unique(original, axis=0, return_inverse=True)
    welded_faces = inverse[faces]
    edges = np.unique(np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]],
                                             welded_faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(points), len(points)))
    count, component = connected_components(adjacency)
    main_component = component == np.bincount(component).argmax()
    points = initial_fit(points)
    points, cuffs = pants.fit_cuffs(points, welded_faces)
    points, waist = fit_waist(points, welded_faces, main_component)
    laplacian = sparse.diags(np.asarray(adjacency.sum(1)).ravel()) - adjacency
    smooth = factorized((sparse.eye(len(points)) + .7 * laplacian).tocsc())
    iterations = []
    for step in range(30):
        correction = np.zeros_like(points)
        correction[main_component], clearance = pants.surface_clearance(points[main_component])
        delta = smooth(correction)
        points += delta
        iterations.append(dict(step=step, minimum_signed_distance_m=float(clearance.min()),
                               maximum_correction_m=float(np.linalg.norm(delta, axis=1).max())))
    points, face_clearance = pants.clear_faces(points, welded_faces, main_component, smooth)
    points, vest_overlap = tuck_belt_under_vest(points, welded_faces, component, main_component)
    joints, weights = pants.skin_weights(points, adjacency, main_component)
    positions = points[inverse]
    normals = io.smooth_normals(positions, faces)
    binary = bytearray(raw)
    primitive['attributes'].update(io.add_skin_attributes(doc, binary, positions, normals,
                                                        joints[inverse], weights[inverse]))
    if 'TANGENT' in primitive['attributes']:
        primitive['attributes']['TANGENT'] = io.add_accessor(doc, binary,
            io.tangents(positions, normals, uv, faces), 'VEC4')
    doc['meshes'][0]['name'] = 'pants_ranger_tripo'
    fit.with_rig(doc, binary, 'pants_ranger', 'tripo_ranger_pants_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'pants_ranger.glb'
    fit.write_glb(target, doc, binary)
    assert np.array_equal(uv, io.accessor(doc, binary, primitive['attributes']['TEXCOORD_0']))
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    report = dict(status='Fitted candidate; dynamic and waist review required after rebuilding',
                  source=sources['source'], base=sources['base'], interfaces=sources['interfaces'],
                  rig_id=sources['rig_id'], source_triangles=len(faces),
                  preserved_source_topology_uv_and_embedded_texture=True,
                  method='Ranger crotch and knee alignment; current v1 ankle sections, radial waist fit and surface-transferred leg weights',
                  anatomical_mapping=dict(source_y=[0, .40, .75, .99951171875], target_y_m=[.181, .553, .870, 1.150]),
                  welded_components=count, rigid_hip_accessory_vertices=int((~main_component).sum()),
                  cuffs=cuffs, waist=waist, ankle_connections=pants.ankle_references(),
                  vest_overlap=vest_overlap,
                  clearance_iterations=iterations, face_clearance=face_clearance,
                  validation=fit.validate(target),
                  remaining_review=['Waist overlap with ranger vest', 'Posed silhouette and skin coverage',
                                    'Other tops and boots; production gameplay'])
    (ROOT / 'doc/assets/modular-ranger-tripo-pants-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation'], indent=2))


if __name__ == '__main__':
    main()
