"""Fit priest chainmail trousers to the canonical male body and rig."""
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy import sparse
from scipy.sparse.csgraph import connected_components
from scipy.sparse.linalg import factorized

from lib.glb import view_bytes
from outfits.rogue_layers import clip_garment, edge_loops

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/priest_tripo_pants_v1'
spec = importlib.util.spec_from_file_location('pants', ROOT / 'tools/fit-tripo-pants.py')
pants = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pants)
fit, io = pants.fit, pants.io


def initial_fit(points):
    result = points.astype(float).copy()
    result[:, 1] = np.interp(points[:, 1], [.09, .22, .40, .628, .99951171875], [.181, .3856, .553, .862, 1.14])
    waist = io.smoothstep((result[:, 1] - .90) / .18)
    result[:, 0] *= .85 + .23 * waist
    result[:, 2] = points[:, 2] * (.95 + .20 * waist) - .027
    return result


def rear_projector(triangles):
    a, b, c = np.moveaxis(triangles, 1, 0)
    ab, ac = b[:, :2] - a[:, :2], c[:, :2] - a[:, :2]
    determinant = ab[:, 0] * ac[:, 1] - ab[:, 1] * ac[:, 0]
    valid = abs(determinant) > 1e-10
    a, b, c, ab, ac, determinant = [v[valid] for v in (a, b, c, ab, ac, determinant)]

    def rear_depth(point):
        offset = point[:2] - a[:, :2]
        u = (offset[:, 0] * ac[:, 1] - offset[:, 1] * ac[:, 0]) / determinant
        v = (ab[:, 0] * offset[:, 1] - ab[:, 1] * offset[:, 0]) / determinant
        hit = (u >= -1e-7) & (v >= -1e-7) & (u + v <= 1 + 1e-7)
        if not hit.any():
            return None
        return (a[:, 2] + u * (b[:, 2] - a[:, 2]) + v * (c[:, 2] - a[:, 2]))[hit].min()

    return rear_depth


def flatten_seat(points, cloth_faces):
    result = points.copy()
    body, faces, _ = fit.body_surface(('torso', 'legs'))
    rear_depth = rear_projector(body[faces])
    reference = io.PARTS / 'fitted/pants_plate.glb'
    doc, raw = fit.read_glb(reference)
    triangles = []
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            vertices = io.accessor(doc, raw, primitive['attributes']['POSITION'])
            indices = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
            triangles.extend(vertices[indices])
    plate_depth = rear_projector(np.asarray(triangles))
    blend = io.smoothstep((points[:, 1] - .76) / .10)
    blend *= io.smoothstep((-points[:, 2] - .005) / .06)
    blend *= 1 - io.smoothstep((abs(points[:, 0]) - .13) / .07)
    for index in np.flatnonzero(blend):
        samples, weights = [], []
        for dx, wx in [(-.012, 1), (0, 2), (.012, 1)]:
            for dy, wy in [(-.012, 1), (0, 2), (.012, 1)]:
                query = points[index] + [dx, dy, 0]
                rear = plate_depth(query)
                if rear is not None and rear < 0:
                    samples.append(rear)
                    weights.append(wx * wy)
        if samples:
            target = -.025 + .9 * (np.average(samples, weights=weights) + .025)
            result[index, 2] += (target - points[index, 2]) * blend[index]
    faces = cloth_faces[np.any(blend[cloth_faces] > 0, axis=1)]
    sample_weights = np.array([[1 / 3, 1 / 3, 1 / 3], [.5, .5, 0], [0, .5, .5], [.5, 0, .5]])
    clearance_steps = []
    for step in range(8):
        correction = np.zeros(len(points))
        for face in faces:
            for weights in sample_weights:
                sample = weights @ result[face]
                if sample[2] >= -.045:
                    continue
                rear = rear_depth(sample)
                if rear is None or rear >= 0:
                    continue
                deficit = min(0, rear - .004 - sample[2])
                movable = weights * (blend[face] > 0)
                divisor = movable @ movable
                if divisor > 0:
                    np.minimum.at(correction, face, deficit * movable / divisor)
        result[:, 2] += correction
        clearance_steps.append(float(-correction.min()))
        if clearance_steps[-1] < 1e-5:
            break
    return result, dict(method='Loft the rear waist, seat and upper thigh along the smoothed plate trouser rear surface, with 90-percent rear depth about Z=-0.025m; retain body clearance',
                        reference=dict(path=str(reference.relative_to(ROOT)), sha256=fit.digest(reference)),
                        smoothing_xy_m=.012, rear_depth_scale=.9,
                        affected_welded_vertices=int(np.any(result != points, axis=1).sum()), transition_y_m=[.76, .86], waist_fade_out=False,
                        face_sample_clearance_m=.004, face_clearance_corrections_m=clearance_steps,
                        maximum_depth_reduction_m=float((result[:, 2] - points[:, 2]).max()),
                        maximum_rear_expansion_m=float((points[:, 2] - result[:, 2]).max()))


def main():
    sources = json.loads((ROOT / 'doc/assets/modular-priest-tripo-pants-sources.json').read_text())
    for source in [sources['source'], sources['base'], sources['interfaces']]:
        assert fit.digest(ROOT / source['path']) == source['sha256']
    doc, raw = fit.read_glb(OUTPUT / 'source.glb')
    assert len(doc['meshes']) == 1 and len(doc['meshes'][0]['primitives']) == 1
    primitive = doc['meshes'][0]['primitives'][0]
    original = io.accessor(doc, raw, primitive['attributes']['POSITION'])
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    source_triangles = len(faces)
    waist_center = (abs(original[:, 0]) < .001) & (abs(original[:, 1] - .86962890625) < 1e-6) & (abs(original[:, 2]) < .005)
    ankle_centers = (abs(abs(original[:, 0]) - .167236328125) < .001) & (abs(original[:, 1] - .0625) < 1e-6) & (abs(original[:, 2] + .058837890625) < .001)
    waist_caps = np.any(waist_center[faces], axis=1)
    ankle_caps = np.any(ankle_centers[faces], axis=1)
    assert waist_caps.sum() == 30 and ankle_caps.sum() == 42
    removed_faces = np.where(waist_caps | ankle_caps)[0]
    faces = faces[~(waist_caps | ankle_caps)]
    removed_cuff_faces = np.flatnonzero(np.all(original[faces, 1] < .09, axis=1))
    split_cuff_faces = np.flatnonzero((original[faces, 1].min(1) < .09) & (original[faces, 1].max(1) > .09))
    original, faces, uv = clip_garment(original, faces, uv, .09 - original[:, 1])
    _, used, remapped = np.unique(np.round(np.column_stack([original, uv]), 7), axis=0,
                                  return_index=True, return_inverse=True)
    original, uv, faces = original[used], uv[used], remapped[faces]
    _, first, inverse = np.unique(np.round(original, 7), axis=0, return_index=True, return_inverse=True)
    points = original[first].copy()
    welded_faces = inverse[faces]
    loops = edge_loops(welded_faces)
    assert len(loops) == 3
    cuff_loops = [loop for loop in loops if points[loop, 1].max() < .091]
    assert len(cuff_loops) == 2
    edges = np.unique(np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]], welded_faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(points), len(points)))
    count, component = connected_components(adjacency)
    main_component = component == np.bincount(component).argmax()
    assert count == 1
    points = initial_fit(points)
    points, cuffs = pants.fit_cuffs(points, welded_faces)
    points, waist = pants.fit_waist(points, welded_faces, main_component, maximum_y=1.095, clearance=.009)
    laplacian = sparse.diags(np.asarray(adjacency.sum(1)).ravel()) - adjacency
    smooth = factorized((sparse.eye(len(points)) + .7 * laplacian).tocsc())
    iterations = []
    for step in range(30):
        correction, clearance = pants.surface_clearance(points)
        delta = smooth(correction)
        points += delta
        iterations.append(dict(step=step, minimum_signed_distance_m=float(clearance.min()),
                               maximum_correction_m=float(np.linalg.norm(delta, axis=1).max())))
    points, face_clearance = pants.clear_faces(points, welded_faces, main_component, smooth, fixed_below=.215)
    points, seat = flatten_seat(points, welded_faces)
    joints, weights = pants.skin_weights(points, adjacency, main_component)
    positions = points[inverse]
    normals = io.smooth_normals(positions, faces)
    binary = bytearray(raw)
    primitive['attributes'].update(io.add_skin_attributes(doc, binary, positions, normals, joints[inverse], weights[inverse]))
    primitive['attributes']['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    primitive['indices'] = io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125)
    if 'TANGENT' in primitive['attributes']:
        primitive['attributes']['TANGENT'] = io.add_accessor(doc, binary, io.tangents(positions, normals, uv, faces), 'VEC4')
    doc['meshes'][0]['name'] = 'pants_priest_tripo'
    fit.with_rig(doc, binary, 'pants_priest', 'tripo_priest_pants_v4')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'pants_priest.glb'
    fit.write_glb(target, doc, binary)
    assert np.array_equal(uv, io.accessor(doc, binary, primitive['attributes']['TEXCOORD_0']))
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    report = dict(status='Fitted candidate; animation and visual review required after rebuilding',
                  source=sources['source'], base=sources['base'], interfaces=sources['interfaces'], rig_id=sources['rig_id'],
                  source_triangles=source_triangles, preserved_retained_vertex_uv_and_embedded_texture=True,
                  revision=4, seat=seat,
                  topology_changes=dict(removed_waist_cap_triangles=int(waist_caps.sum()), removed_ankle_cap_triangles=int(ankle_caps.sum()),
                                        removed_source_cap_face_indices=removed_faces.tolist(),
                                        removed_cuff_triangles_after_caps=len(removed_cuff_faces), split_cuff_triangles=len(split_cuff_faces),
                                        source_cuff_cutoff_y=.09, open_loops=len(loops),
                                        method='Remove recessed caps and black folded cuffs; interpolate original UVs at clean open ankle cuts, then extend the remaining chainmail to the original fitted hem height'),
                  anatomical_mapping=dict(source_y=[.09, .22, .40, .628, .99951171875], target_y_m=[.181, .3856, .553, .862, 1.14]),
                  welded_components=count, cuffs=cuffs, waist=waist, ankle_connections=pants.ankle_references(),
                  clearance_iterations=iterations, face_clearance=face_clearance, validation=fit.validate(target),
                  remaining_review=['Posed garment/body intersections', 'Priest top overlap', 'Other tops and boots'])
    (ROOT / 'doc/assets/modular-priest-tripo-pants-fitting-v4.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation'], indent=2))


if __name__ == '__main__':
    main()
