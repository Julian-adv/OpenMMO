"""Fit the preserved Tripo vest to the canonical modular male and its existing rig."""
import importlib.util
import json
import sys
from io import BytesIO
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import sparse
from scipy.ndimage import gaussian_filter
from scipy.sparse.linalg import factorized

from lib.glb import view_bytes
from outfits.rogue_layers import compact_weights, wrist_section

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
spec = importlib.util.spec_from_file_location('rogue_fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/rogue_tripo_v1'
REPORT = ROOT / 'doc/assets/modular-rogue-tripo-fitting-v1.json'


def initial_fit(points):
    result = points.astype(float).copy()
    result[:, 0] *= .80
    result[:, 1] = np.interp(points[:, 1], [0, .3, .6, .864], [1.05, 1.26, 1.49, 1.66])
    result[:, 2] = points[:, 2] * .65 - .035
    return result


def fit_waist(points, faces, joints, weights):
    plate = ROOT / 'assets/modular_human_male_01/parts/fitted/top_plate.glb'
    doc, binary = fit.read_glb(plate)
    triangles = np.concatenate([
        io.accessor(doc, binary, primitive['attributes']['POSITION'])[
            io.accessor(doc, binary, primitive['indices']).reshape(-1, 3)]
        for mesh in doc['meshes'] for primitive in mesh['primitives']
    ])
    surface = fit.body_surface(('torso', 'legs', 'upper_arms'))
    heights = np.linspace(1.10, 1.30, 21)
    theta = np.arange(96) * 2 * np.pi / 96
    radial = np.column_stack([np.sin(theta), np.cos(theta)])
    spine = np.array([fit.BONES[name] for name in ['Hips', 'Spine', 'Spine1', 'Spine2']])
    centers = np.interp(heights, spine[:, 1], spine[:, 2])
    plate_extents, body_extents, cloth_extents = [], [], []
    sections = [(triangles, plate_extents), (surface[0][surface[1]], body_extents),
                (points[faces], cloth_extents)]
    for height, center in zip(heights, centers):
        args = (np.array([0, height, 0]), np.array([0, 1, 0]),
                np.array([[1, 0, 0], [0, 0, 1]]), radial)
        for mesh, result in sections:
            _, radius = wrist_section(mesh, *args, mid=np.array([0, center]), outermost=False)
            ring = radial * radius[:, None]
            result.append(ring.max(axis=0) - ring.min(axis=0))
    target = np.maximum(plate_extents, np.array(body_extents) + .030)
    scales = np.minimum(target / np.array(cloth_extents), 1)
    scales = gaussian_filter(scales, (2, 0), mode='nearest')
    scale = np.column_stack([np.interp(points[:, 1], heights, scales[:, i]) for i in range(2)])
    center = np.interp(points[:, 1], heights, centers)
    offset = points[:, [0, 2]] - np.column_stack([np.zeros(len(points)), center])
    arm_indices = [i for i, name in enumerate(fit.NAMES) if name.endswith(('Arm', 'ForeArm'))]
    arms = np.sum(weights * np.isin(joints, arm_indices), axis=1)
    amount = .85 * (1 - io.smoothstep((points[:, 1] - 1.20) / .16))
    amount *= 1 - io.smoothstep((abs(points[:, 0]) - .22) / .07)
    amount *= 1 - io.smoothstep((arms - .25) / .25)
    result = points.copy()
    result[:, [0, 2]] -= offset * (1 - scale) * amount[:, None]
    displacement = np.linalg.norm(result - points, axis=1)
    return result, dict(
        method='Smooth section scales follow the plate waist silhouette with body clearance; shared layer deformation preserves cloth overlap',
        reference=dict(path=str(plate.relative_to(ROOT)), sha256=fit.digest(plate)),
        maximum_contour_blend=.85, transition_height_m=[1.20, 1.36],
        section_body_margin_m=.030,
        changed_vertices=int((displacement > 0).sum()),
        maximum_displacement_m=float(np.linalg.norm(result - points, axis=1).max()),
        original_heights_and_skin_weights_preserved=True,
    )


def stiffen_leather(points, dense, laplacian):
    amount = .94 * (1 - io.smoothstep((abs(points[:, 0]) - .16) / .12))
    amount *= 1 - io.smoothstep((points[:, 1] - 1.43) / .09)
    amount *= io.smoothstep((points[:, 1] - 1.12) / .10)
    result = dense * (1 - amount[:, None])
    result[:, fit.NAMES.index('Spine1')] += amount
    core = (abs(points[:, 0]) < .14) & (points[:, 1] > 1.23) & (points[:, 1] < 1.43)
    fidelity = np.where(core, 200., 2.)
    matrix = (sparse.diags(fidelity) + .04 * laplacian).tocsr()
    moving, fixed = amount > 0, amount == 0
    rhs = result[moving] * fidelity[moving, None] - matrix[moving][:, fixed] @ dense[fixed]
    result[moving] = factorized(matrix[moving][:, moving].tocsc())(rhs)
    result[fixed] = dense[fixed]
    result = np.maximum(result, 0)
    return result / result.sum(1, keepdims=True)


def ease_right_armhole(points):
    x, y, z = points.T
    amount = io.smoothstep((y - 1.34) / .10) * (1 - io.smoothstep((y - 1.465) / .04))
    amount *= io.smoothstep((-x - .13) / .04) * (1 - io.smoothstep((-x - .215) / .055))
    amount *= io.smoothstep((z - .015) / .03)
    result = points.copy()
    result[:, 0] -= .005 * amount
    return result, dict(
        method='Ease the character-right front armhole outward with a smooth local falloff',
        maximum_displacement_m=float(.005 * amount.max()), changed_vertices=int((amount > 0).sum()),
        unchanged_uv_texture_skin_weights=True, applies_to='Base garment shape in all poses',
    )


def bind_side_laces(points, faces, joints, weights, uv, inverse, texture, laplacian):
    image = np.asarray(Image.open(BytesIO(texture)))
    pixels = image[np.clip((uv[:, 1] * image.shape[0]).astype(int), 0, image.shape[0] - 1),
                   np.clip((uv[:, 0] * image.shape[1]).astype(int), 0, image.shape[1] - 1), :3].astype(float)
    leather = (pixels[:, 0] > pixels[:, 1] * 1.28) & (pixels[:, 0] > pixels[:, 2] * 1.55)
    votes = np.bincount(inverse, weights=leather, minlength=len(points))
    leather = votes / np.bincount(inverse, minlength=len(points)) > .5
    x, y, z = points.T
    patch = leather & (abs(x) > .16) & (abs(x) < .235) & (y > 1.10) & (y < 1.39) & (z > -.09) & (z < .045)
    dense = np.zeros((len(points), len(fit.NAMES)))
    np.add.at(dense, (np.arange(len(points))[:, None], joints), weights)
    original_dense = dense.copy()
    donor_faces = faces[(abs(points[faces, 0]) < .16).all(1) & (points[faces, 1].mean(1) < 1.43)]
    _, _, transferred = fit.nearest_surface(points[patch], (points, donor_faces, dense), candidates=128)
    amount = io.smoothstep((abs(x[patch]) - .16) / .025)
    amount *= io.smoothstep((y[patch] - 1.10) / .03) * (1 - io.smoothstep((y[patch] - 1.35) / .04))
    dense[patch] = dense[patch] * (1 - amount[:, None]) + transferred * amount[:, None]
    moving = (abs(x) > .15) & (abs(x) < .25) & (y > 1.08) & (y < 1.41) & (z > -.11) & (z < .06)
    fidelity = np.where(patch, 80., 2.)
    matrix = (sparse.diags(fidelity) + .04 * laplacian).tocsr()
    rhs = dense[moving] * fidelity[moving, None] - matrix[moving][:, ~moving] @ original_dense[~moving]
    dense[moving] = np.maximum(factorized(matrix[moving][:, moving].tocsc())(rhs), 0)
    upper = io.smoothstep((y[moving] - 1.18) / .05)
    for name in ['Hips', 'Spine']:
        index = fit.NAMES.index(name)
        shifted = dense[moving, index] * upper
        dense[moving, index] -= shifted
        dense[moving, fit.NAMES.index('Spine1')] += shifted
    updated_joints, updated_weights = compact_weights(dense[moving])
    joints, weights = joints.copy(), weights.copy()
    joints[moving], weights[moving] = updated_joints, updated_weights
    return joints, weights, dict(
        method='Transfer adjacent vest panel weights to leather side laces and their attachments',
        changed_welded_vertices=int(moving.sum()), lace_welded_vertices=int(patch.sum()),
        attachment_transition_diffusion=.04, unchanged_geometry_uv_texture=True,
    )


def main():
    source = OUTPUT / 'source.glb'
    base = ROOT / 'assets/modular_human_male_01/parts/fitted/base.glb'
    sources = json.loads((ROOT / 'doc/assets/modular-rogue-tripo-sources.json').read_text())
    assert fit.digest(base) == sources['base']['sha256']
    assert fit.digest(source) == sources['source']['sha256']
    doc, raw = fit.read_glb(source)
    assert len(doc['meshes']) == 1 and len(doc['meshes'][0]['primitives']) == 1
    primitive = doc['meshes'][0]['primitives'][0]
    original_uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    original_images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    original = io.accessor(doc, raw, primitive['attributes']['POSITION'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    points, unique_index, inverse = np.unique(original, axis=0, return_index=True, return_inverse=True)
    welded_faces = inverse[faces]
    points = initial_fit(points)
    edges = np.unique(np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]], welded_faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(points), len(points)))
    laplacian = sparse.diags(np.asarray(adjacency.sum(1)).ravel()) - adjacency
    smooth = factorized((sparse.eye(len(points)) + 2.5 * laplacian).tocsc())
    surface = fit.body_surface(('torso', 'neck', 'upper_arms', 'forearms'))
    iterations = []
    for step in range(12):
        nearest, normals, _ = fit.nearest_surface(points, surface, candidates=64)
        signed = np.sum((points - nearest) * normals, axis=1)
        correction = normals * np.clip(.012 - signed, 0, .025)[:, None]
        correction = np.column_stack([smooth(correction[:, i]) for i in range(3)])
        points += correction
        iterations.append(dict(step=step, minimum_signed_distance_m=float(signed.min()), maximum_correction_m=float(np.linalg.norm(correction, axis=1).max())))
    positions = points[inverse]
    _, _, dense = fit.nearest_surface(points, surface, candidates=64)
    for side in ['Left', 'Right']:
        shoulder = fit.NAMES.index(side + 'Shoulder')
        dense[:, fit.NAMES.index('Spine2')] += dense[:, shoulder] * .5
        dense[:, fit.NAMES.index(side + 'Arm')] += dense[:, shoulder] * .5
        dense[:, shoulder] = 0
    allowed = {'Hips', 'Spine', 'Spine1', 'Spine2', 'Neck', 'LeftArm', 'LeftForeArm', 'RightArm', 'RightForeArm'}
    dense[:, [i for i, name in enumerate(fit.NAMES) if name not in allowed]] = 0
    edge_weights = 1 / np.maximum(np.linalg.norm(points[rows] - points[cols], axis=1), .001) ** 2
    weighted = sparse.csr_matrix((edge_weights, (rows, cols)), shape=adjacency.shape)
    laplacian = sparse.diags(np.asarray(weighted.sum(1)).ravel()) - weighted
    cuff = io.smoothstep((abs(points[:, 0]) - .23) / .07) * (1 - io.smoothstep((points[:, 1] - 1.27) / .10))
    sleeve = io.smoothstep((abs(points[:, 0]) - .22) / .07)
    fidelity = 1 + cuff * 80 + sleeve * 20
    smooth_weights = factorized((sparse.diags(fidelity) + .05 * laplacian).tocsc())
    dense = np.maximum(np.column_stack([smooth_weights(dense[:, i] * fidelity) for i in range(len(fit.NAMES))]), 0)
    dense = np.maximum(dense - .015, 0)
    dense /= dense.sum(1, keepdims=True)
    local = np.zeros_like(dense)
    for side, sign in [('Left', 1), ('Right', -1)]:
        for bone in ['Arm', 'ForeArm']:
            index = fit.NAMES.index(side + bone)
            local[:, index] = dense[:, index] * io.smoothstep((points[:, 0] * sign) / .13)
    trunk = 1 - local.sum(1)
    spine = ['Hips', 'Spine', 'Spine1', 'Spine2', 'Neck']
    levels = np.array([fit.BONES[name][1] for name in spine])
    lower = np.clip(np.searchsorted(levels, points[:, 1]) - 1, 0, len(spine) - 2)
    blend = io.smoothstep((points[:, 1] - levels[lower]) / (levels[lower + 1] - levels[lower]))
    local[np.arange(len(points)), [fit.NAMES.index(spine[i]) for i in lower]] = trunk * (1 - blend)
    local[np.arange(len(points)), [fit.NAMES.index(spine[i + 1]) for i in lower]] = trunk * blend
    dense = local
    assert (dense.sum(1) > 0).all()
    joints, weights = compact_weights(dense)
    positions, waist = fit_waist(positions, faces, joints[inverse], weights[inverse])
    dense = stiffen_leather(points, dense, laplacian)
    joints, weights = compact_weights(dense)
    positions, armhole = ease_right_armhole(positions)
    joints, weights, laces = bind_side_laces(positions[unique_index], welded_faces, joints, weights,
                                           original_uv, inverse, original_images[0], laplacian)
    binary = bytearray(raw)
    primitive['attributes'].update(io.add_skin_attributes(doc, binary, positions, io.smooth_normals(positions, faces), joints[inverse], weights[inverse]))
    doc['meshes'][0]['name'] = 'top_rogue_tripo'
    fit.with_rig(doc, binary, 'top_rogue', 'tripo_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'top_rogue.glb'
    fit.write_glb(target, doc, binary)
    assert np.array_equal(original_uv, io.accessor(doc, binary, doc['meshes'][0]['primitives'][0]['attributes']['TEXCOORD_0']))
    assert original_images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    report = dict(
        status='Fitted and rigged candidate; rerun animation and visual review after rebuilding',
        source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
        base=dict(path=str(base.relative_to(ROOT)), sha256=fit.digest(base)),
        rig_id='human_male_01_mixamo_candidate_v2',
        method='Anatomical alignment, smooth clearance correction, plate waist contour, barycentric sleeve weights and a shared leather torso blend',
        source_triangles=len(faces), preserved_uv_and_embedded_texture=True,
        clearance_target_m=.012, correction_iterations=iterations, validation=fit.validate(target),
        waist_taper=waist, right_armhole=armhole, side_laces=laces,
    )
    samples = np.concatenate([positions, positions[faces].mean(1)])
    nearest, normals, _ = fit.nearest_surface(samples, surface, candidates=128)
    signed = np.sum((samples - nearest) * normals, axis=1)
    report['rest_surface_diagnostic'] = dict(samples=len(samples), minimum_signed_distance_m=float(signed.min()), negative_samples=int((signed < 0).sum()), interpretation='Includes retained internal collar and cuff surfaces; not a collision-free acceptance test')
    report['weight_policy'] = dict(surface_diffusion=.05, cuff_fidelity=80, sleeve_fidelity=20, maximum_influences=4, arm_centerline_fade_m=.13,
                                 leather_shared_blend=dict(Spine1=1), leather_stiffness=.94, leather_transition_diffusion=.04,
                                 leather_side_transition_abs_x_m=[.16, .28], leather_upper_transition_y_m=[1.43, 1.52],
                                 leather_hem_transition_y_m=[1.12, 1.22])
    report['budget'] = dict(top_triangles=len(faces), body_with_hidden_regions=13891, crop_hair=933, body_hair_top_total=13891 + 933 + len(faces), full_rogue_set_with_hair_sword_including_hidden=23458, face_triangles=1505)
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation'], indent=2))


if __name__ == '__main__':
    main()
