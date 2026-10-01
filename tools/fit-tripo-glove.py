"""Fit the Tripo right glove while preserving the existing left wrist wrap."""
import copy
import importlib.util
from io import BytesIO
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import sparse
from scipy.interpolate import LinearNDInterpolator, RBFInterpolator
from scipy.spatial import Delaunay, cKDTree
from scipy.sparse.csgraph import connected_components

from lib.glb import view_bytes
from outfits.rogue_layers import clip_garment, compact_weights, edge_loops, wrist_section

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/rogue_tripo_glove_v1'
spec = importlib.util.spec_from_file_location('rogue_fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io
ALONG = io.unit(np.array([0, -.874, .486]))
DEPTH = io.unit(np.array([0, .486, .874]))
FINGERS = ['Pinky', 'Ring', 'Middle', 'Index', 'Thumb']
CENTERS = np.array([-.338, -.206, -.034, .161, .331])
ROOT_HEIGHTS = np.array([-.30, -.18, -.17, -.18, -.48])


def local(points):
    return np.column_stack([points[:, 0], points @ ALONG, points @ DEPTH])


def topology(points, faces):
    unique, inverse = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    welded = inverse[faces]
    edges = np.unique(np.sort(np.concatenate([welded[:, [0, 1]], welded[:, [1, 2]], welded[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(unique), len(unique)))
    return unique, inverse, welded, adjacency


def open_fingers(points, faces, uv):
    q = local(points)
    cutoff = np.interp(q[:, 0], [-.5, -.29, -.26, -.105, -.085, .055, .075, .245, .28, .5],
        [-.20, -.20, -.02, -.02, .035, .035, .01, .01, -.30, -.30])
    points, faces, uv = clip_garment(points, faces, uv, q[:, 1] - cutoff)
    unique, inverse, _, adjacency = topology(points, faces)
    _, component = connected_components(adjacency)
    main = component == np.bincount(component).argmax()
    faces = faces[np.all(main[inverse[faces]], axis=1)]
    used, remapped = np.unique(faces, return_inverse=True)
    points, uv, faces = points[used], uv[used], remapped.reshape(-1, 3)
    vertices, inverse, welded, _ = topology(points, faces)
    loops = edge_loops(welded)
    assert len(loops) == 6, 'Five finger openings and one wrist opening required'
    assert sorted(len(loop) for loop in loops)[0] >= 20
    return points, faces, uv, [local(vertices[loop]).mean(0) for loop in loops]


def align(points, source_faces, holes):
    q = local(points)
    wrist = fit.BONES['RightHand']
    axis = io.unit(wrist - fit.BONES['RightForeArm'])
    width = fit.BONES['RightHandIndex1'] - fit.BONES['RightHandPinky1']
    width = io.unit(width - axis * (width @ axis))
    normal = io.unit(np.cross(axis, width))
    source = [[.08, -.93], [.06, -.68], [-.015, -.47]]
    target = [wrist - axis * .04, wrist, wrist + axis * .055]
    for height, along, center, radius in [(-.93, -.04, .08, .035), (-.68, 0, .06, .038), (-.47, .055, -.015, .048)]:
        for sign in [-1, 1]:
            source.append([center + sign * .23, height])
            target.append(wrist + axis * along + width * sign * radius)
    fingers = {}
    for i, finger in enumerate(FINGERS):
        hole = min((hole for hole in holes if hole[1] > -.7), key=lambda hole: abs(hole[0] - CENTERS[i]))
        root, middle, end = [fit.BONES[f'RightHand{finger}{j}'] for j in [1, 2, 3]]
        source.extend([[CENTERS[i], ROOT_HEIGHTS[i]], hole[:2].tolist()])
        target.extend([root, middle * .85 + end * .15])
        fingers[finger] = dict(source_root=[float(CENTERS[i]), float(ROOT_HEIGHTS[i])], source_opening=hole.tolist(),
            target_root=root.tolist(), target_opening=(middle * .85 + end * .15).tolist())
    warp = RBFInterpolator(np.array(source), np.array(target), kernel='thin_plate_spline', smoothing=1e-7)
    out = warp(q[:, :2])
    centers = points - (points @ DEPTH)[:, None] * DEPTH
    lo, hi = fit.ray_bounds(centers, DEPTH, source_faces, 2.)
    source_hit = np.isfinite(lo)
    if not source_hit.all():
        nearest = cKDTree(q[source_hit, :2]).query(q[~source_hit, :2])[1]
        lo[~source_hit] = lo[source_hit][nearest]
        hi[~source_hit] = hi[source_hit][nearest]
    center, radius = (hi + lo) / 2, (hi - lo) / 2
    surface = fit.body_surface(('hands', 'forearms'), -1)
    low, high = fit.ray_bounds(out, normal, surface[0][surface[1]], .13)
    hit = np.isfinite(low)
    body_center = np.zeros(len(out))
    thickness = np.where(q[:, 1] > -.35, .013, .023)
    body_center[hit] = (high[hit] + low[hit]) / 2
    thickness[hit] = (high[hit] - low[hit]) / 2 + .006
    out += (body_center + np.clip((points @ DEPTH - center) / radius, -1.3, 1.3) * thickness)[:, None] * normal
    return out, dict(method='Source cuff, palm and five opening landmarks aligned to actual canonical right-hand bones; source/body ray sections set palm thickness',
        fingers=fingers, source_axis=ALONG.tolist(), source_depth_axis=DEPTH.tolist(), body_depth_axis=normal.tolist(),
        source_ray_section_hits=int(source_hit.sum()), source_boundary_section_fallbacks=int((~source_hit).sum()), ray_section_hits=int(hit.sum()),
        source_landmarks=source, target_landmarks=np.asarray(target).tolist())


def contour_fit(source, positions, source_triangles, alignment):
    surface = fit.body_surface(('hands', 'forearms'), -1)
    skin_triangles = surface[0][surface[1]]
    q = local(source)
    theta = np.arange(96) * 2 * np.pi / 96
    radial = np.column_stack([np.cos(theta), np.sin(theta)])
    wrist = fit.BONES['RightHand']
    axis = io.unit(wrist - fit.BONES['RightForeArm'])
    normal = np.array(alignment['body_depth_axis'])
    width = io.unit(np.cross(normal, axis))
    result = positions.copy()

    def fit_sections(mask, source_origin, source_axis, source_width, source_heights, target_origin, target_axis, target_width, target_heights, amount):
        source_basis = np.array([source_width, np.cross(source_axis, source_width)])
        target_basis = np.array([target_width, np.cross(target_axis, target_width)])
        centers, radii, target_centers, target_radii = [], [], [], []
        for height, target_height in zip(source_heights, target_heights):
            center, radius = wrist_section(source_triangles, source_origin + source_axis * height, source_axis, source_basis, radial, outermost=False)
            target_center, target_radius = wrist_section(skin_triangles, target_origin + target_axis * target_height, target_axis, target_basis, radial, outermost=False)
            centers.append(center)
            radii.append(radius)
            target_centers.append(target_center)
            target_radii.append(target_radius)
        rows = np.where(mask)[0]
        points = source[rows] - source_origin
        heights = points @ source_axis
        centers, radii, target_centers, target_radii = map(np.asarray, [centers, radii, target_centers, target_radii])
        center = np.column_stack([np.interp(heights, source_heights, centers[:, i]) for i in range(2)])
        cross = points @ source_basis.T - center
        angles = np.arctan2(cross[:, 1], cross[:, 0])
        radius = np.linalg.norm(cross, axis=1)

        def sample(table):
            values = np.array([np.interp(angles, theta, row, period=2 * np.pi) for row in table])
            return np.array([np.interp(height, source_heights, values[:, i]) for i, height in enumerate(heights)])

        fitted_radius = (sample(target_radii) + .0025) * np.clip(radius / sample(radii), 1, 1.12)
        target_center = np.column_stack([np.interp(heights, source_heights, target_centers[:, i]) for i in range(2)])
        target_height = np.interp(heights, source_heights, target_heights)
        target_height += np.where(heights < source_heights[0], (heights - source_heights[0]) * (target_heights[1] - target_heights[0]) / (source_heights[1] - source_heights[0]), 0)
        target_height += np.where(heights > source_heights[-1], (heights - source_heights[-1]) * (target_heights[-1] - target_heights[-2]) / (source_heights[-1] - source_heights[-2]), 0)
        target = target_origin + target_height[:, None] * target_axis
        target += (target_center + fitted_radius[:, None] * np.column_stack([np.cos(angles), np.sin(angles)])) @ target_basis
        result[rows] = result[rows] * (1 - amount[rows, None]) + target * amount[rows, None]

    heights = np.linspace(-.82, -.42, 15)
    fit_sections(q[:, 1] < -.40, np.zeros(3), ALONG, np.array([1, 0, 0]), heights,
        wrist, axis, width, np.interp(heights, [-.93, -.68, -.40], [-.04, 0, .095]),
        1 - io.smoothstep((q[:, 1] + .48) / .08))
    return result


def hand_shell():
    body, faces, dense = fit.body_surface(('hands', 'forearms'), -1)
    positions = body + io.smooth_normals(body, faces) * .003
    wrist = fit.BONES['RightHand']
    axis = io.unit(wrist - fit.BONES['RightForeArm'])
    distance = -.02 - (positions - wrist) @ axis
    field = np.array([dense[:, [j for j, name in enumerate(fit.NAMES) if name.startswith('RightHand' + finger)]].sum(1) for finger in FINGERS]).T
    labels = field.argmax(1)
    for i, finger in enumerate(FINGERS):
        root, middle, end = [fit.BONES[f'RightHand{finger}{j}'] for j in [1, 2, 3]]
        opening = middle * .85 + end * .15
        axis = io.unit(opening - root)
        mask = (labels == i) & (field[:, i] > .05)
        cut = (positions[mask] - opening) @ axis
        if finger == 'Thumb':
            web = fit.BONES['RightHandIndex1'] - root
            web = io.unit(web - axis * (web @ axis))
            cut -= .020 * io.smoothstep(((positions[mask] - opening) @ web - .012) / .016)
        distance[mask] = np.maximum(distance[mask], cut)
    points, faces, payload = clip_garment(positions, faces, np.column_stack([np.zeros((len(positions), 2)), dense]), distance)
    dense = payload[:, 2:]
    points, dense = points[faces].reshape(-1, 3), dense[faces].reshape(-1, len(fit.NAMES))
    faces = np.arange(len(points)).reshape(-1, 3)
    joints, skin = compact_weights(dense)
    unique, inverse, welded, _ = topology(points, faces)
    loops = edge_loops(welded)
    assert len(loops) == 6
    regions = {}
    labels = np.array([dense[:, [j for j, name in enumerate(fit.NAMES) if name.startswith('RightHand' + finger)]].sum(1) for finger in FINGERS]).T.argmax(1)
    for i, finger in enumerate(FINGERS):
        own = [fit.NAMES.index('RightHand')] + [fit.NAMES.index(f'RightHand{finger}{j}') for j in range(1, 5)]
        forbidden = [j for j in range(len(fit.NAMES)) if j not in own]
        rows = np.where((labels == i) & (dense[:, forbidden].sum(1) < 1e-6) & (dense[:, own[1:]].sum(1) > .5))[0]
        regions[finger] = dict(vertices=rows.tolist(), allowed_bones=[fit.NAMES[j] for j in own])
    return points, faces, joints, skin, regions


def bake_texture(shell, faces, source, source_faces, source_uv, image, alignment):
    wrist = fit.BONES['RightHand']
    axis = io.unit(wrist - fit.BONES['RightForeArm'])
    normal = np.array(alignment['body_depth_axis'])
    basis = np.array([io.unit(np.cross(normal, axis)), axis])
    projected = (shell - wrist) @ basis.T
    low, high = projected.min(0) - .002, projected.max(0) + .002
    landmarks = (np.array(alignment['target_landmarks']) - wrist) @ basis.T
    source_landmarks = np.array(alignment['source_landmarks'])
    perimeter, source_perimeter = [], []
    for finger in FINGERS:
        info = alignment['fingers'][finger]
        root, opening = np.array(info['target_root']), np.array(info['target_opening'])
        along = io.unit((opening - root) @ basis.T)
        width = np.array([along[1], -along[0]])
        for key in ['root', 'opening']:
            for sign in [-1, 1]:
                perimeter.append((np.array(info['target_' + key]) - wrist) @ basis.T + width * sign * .010)
                source_perimeter.append(np.array(info['source_' + key])[:2] + [sign * .075, 0])
    landmarks = np.vstack([landmarks, perimeter])
    source_landmarks = np.vstack([source_landmarks, source_perimeter])
    triangulation = Delaunay(landmarks)
    inverse = LinearNDInterpolator(triangulation, source_landmarks)
    raw = local(source)[source_faces]
    tree = cKDTree(raw[:, :, :2].mean(1))
    edges = np.stack([raw[:, 1, :2] - raw[:, 0, :2], raw[:, 2, :2] - raw[:, 0, :2]], axis=-1)
    inverses = np.linalg.pinv(edges)
    texture = np.asarray(Image.open(BytesIO(image)).convert('RGB'))
    atlas = Image.new('RGB', (4096, 2048))
    atlas.paste(Image.fromarray(texture), (0, 0))
    size = 1024
    grid = np.stack(np.meshgrid((np.arange(size) + .5) / size, (np.arange(size) + .5) / size), axis=-1).reshape(-1, 2)
    mapped = inverse(low + grid * (high - low))
    missing = ~np.isfinite(mapped).all(1)
    planar = np.column_stack([landmarks, np.zeros(len(landmarks))])
    missing = np.flatnonzero(missing)
    for start in range(0, len(missing), 8192):
        rows = missing[start:start + 8192]
        queries = np.column_stack([low + grid[rows] * (high - low), np.zeros(len(rows))])
        _, _, mapped[rows] = fit.nearest_surface(queries, (planar, triangulation.simplices, source_landmarks))
    colors = [np.empty((len(grid), 3), dtype=np.uint8) for _ in range(2)]
    for start in range(0, len(grid), 8192):
        queries = mapped[start:start + 8192]
        near = tree.query(queries, k=64)[1]
        coordinates = np.einsum('nkij,nkj->nki', inverses[near], queries[:, None] - raw[near, 0, :2])
        bary = np.concatenate([1 - coordinates.sum(-1, keepdims=True), coordinates], axis=-1)
        depths = np.sum(bary * raw[near, :, 2], axis=-1)
        hit = np.min(bary, axis=-1) >= -1e-6
        for side in range(2):
            scores = np.where(hit, depths * (1 if side == 0 else -1), -np.inf)
            selected = scores.argmax(1)
            selected[~hit.any(1)] = 0
            rows = np.arange(len(queries))
            weights = np.clip(bary[rows, selected], 0, 1)
            weights /= weights.sum(1, keepdims=True)
            uv = np.sum(source_uv[source_faces[near[rows, selected]]] * weights[..., None], axis=1)
            pixels = np.clip(uv * [texture.shape[1] - 1, texture.shape[0] - 1], 0, 2047)
            lo = pixels.astype(int)
            hi = np.minimum(lo + 1, 2047)
            blend = pixels - lo
            rgb = ((texture[lo[:, 1], lo[:, 0]] * (1 - blend[:, 0, None]) + texture[lo[:, 1], hi[:, 0]] * blend[:, 0, None]) * (1 - blend[:, 1, None])
                + (texture[hi[:, 1], lo[:, 0]] * (1 - blend[:, 0, None]) + texture[hi[:, 1], hi[:, 0]] * blend[:, 0, None]) * blend[:, 1, None])
            colors[side][start:start + len(queries)] = rgb.clip(0, 255).astype(np.uint8)
    plane = low + grid * (high - low)
    thumb = io.smoothstep((plane[:, 0] - .012) / .02) * (1 - io.smoothstep((plane[:, 1] - .085) / .03))
    leather_x = -.015 + np.clip((plane[:, 0] - .012) / .07, 0, 1) * .025
    leather_y = .04 + np.clip(plane[:, 1] / .12, 0, 1) * .03
    columns = np.clip(((leather_x - low[0]) / (high[0] - low[0]) * size).astype(int), 0, size - 1)
    rows = np.clip(((leather_y - low[1]) / (high[1] - low[1]) * size).astype(int), 0, size - 1)
    for side in range(2):
        chart = colors[side].reshape(size, size, 3)
        leather = chart[rows, columns].copy()
        colors[side] = (colors[side] * (1 - thumb[:, None]) + leather * thumb[:, None]).astype(np.uint8)
        atlas.paste(Image.fromarray(colors[side].reshape(size, size, 3)), (2048, side * size))
    uv = (projected - low) / (high - low)
    chart = (np.cross(shell[faces[:, 1]] - shell[faces[:, 0]], shell[faces[:, 2]] - shell[faces[:, 0]]) @ normal < 0).astype(int)
    uv[:, 0] = .5 + uv[:, 0] * .25
    uv[:, 1] = uv[:, 1] * .5 + np.repeat(chart, 3) * .5
    output = BytesIO()
    atlas.save(output, format='PNG')
    return uv, output.getvalue()


def append_wrap(doc, binary, source):
    old, raw = fit.read_glb(ROOT / source['path'])
    mesh = next(mesh for mesh in old['meshes'] if mesh['name'] == 'wrap_rogue_left')
    primitive = mesh['primitives'][0]
    material = copy.deepcopy(old['materials'][primitive['material']])
    color = material['pbrMetallicRoughness']['baseColorTexture']
    texture = copy.deepcopy(old['textures'][color['index']])
    image = copy.deepcopy(old['images'][texture['source']])
    contents = view_bytes(old, raw, image['bufferView'])
    binary.extend(b'\0' * (-len(binary) % 4))
    image['bufferView'] = len(doc['bufferViews'])
    doc['bufferViews'].append(dict(buffer=0, byteOffset=len(binary), byteLength=len(contents)))
    binary.extend(contents)
    texture['source'] = len(doc['images'])
    doc['images'].append(image)
    if 'sampler' in texture:
        sampler = copy.deepcopy(old['samplers'][texture['sampler']])
        texture['sampler'] = len(doc.setdefault('samplers', []))
        doc['samplers'].append(sampler)
    color['index'] = len(doc['textures'])
    doc['textures'].append(texture)
    new = dict(material=len(doc['materials']), attributes={})
    doc['materials'].append(material)
    for name, index in primitive['attributes'].items():
        accessor = old['accessors'][index]
        new['attributes'][name] = io.add_accessor(doc, binary, io.accessor(old, raw, index), accessor['type'], accessor['componentType'])
        assert np.array_equal(io.accessor(old, raw, index), io.accessor(doc, binary, new['attributes'][name]))
    accessor = old['accessors'][primitive['indices']]
    new['indices'] = io.add_accessor(doc, binary, io.accessor(old, raw, primitive['indices']), accessor['type'], accessor['componentType'])
    assert np.array_equal(io.accessor(old, raw, primitive['indices']), io.accessor(doc, binary, new['indices']))
    assert contents == view_bytes(doc, binary, image['bufferView'])
    doc['meshes'].append(dict(name='wrap_rogue_left', primitives=[new]))


def main():
    sources = json.loads((ROOT / 'doc/assets/modular-rogue-tripo-glove-sources.json').read_text())
    for source in [sources['source'], sources['base'], sources['interfaces'], sources['previous_gloves']]:
        assert fit.digest(ROOT / source['path']) == source['sha256']
    doc, raw = fit.read_glb(OUTPUT / 'source.glb')
    primitive = doc['meshes'][0]['primitives'][0]
    points = io.accessor(doc, raw, primitive['attributes']['POSITION'])
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    source_faces = points[faces]
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    reference, reference_faces, reference_uv, holes = open_fingers(points, faces, uv)
    _, alignment = align(reference, source_faces, holes)
    shell, shell_faces, shell_joints, shell_skin, regions = hand_shell()
    shell_uv, atlas = bake_texture(shell, shell_faces, points, faces, uv, images[0], alignment)
    cuff, cuff_faces, cuff_uv = clip_garment(points, faces, uv, local(points)[:, 1] + .65)
    unique, inverse, _, _ = topology(cuff, cuff_faces)
    cuff_positions, _ = align(unique, source_faces, holes)
    cuff_positions = contour_fit(unique, cuff_positions, source_faces, alignment)
    cuff_uv[:, 0] *= .5
    allowed = {'RightForeArm', 'RightHand'}
    cuff_joints, cuff_skin = fit.transfer(cuff_positions, fit.body_surface(('hands', 'forearms'), -1), allowed)
    positions = np.vstack([shell, cuff_positions[inverse]])
    faces = np.vstack([shell_faces, cuff_faces + len(shell)])
    uv = np.vstack([shell_uv, cuff_uv])
    joints = np.vstack([shell_joints, cuff_joints[inverse]])
    skin = np.vstack([shell_skin, cuff_skin[inverse]])
    assert len(positions) == len(uv) == len(joints) == len(skin)
    binary = bytearray(raw)
    binary.extend(b'\0' * (-len(binary) % 4))
    doc['bufferViews'].append(dict(buffer=0, byteOffset=len(binary), byteLength=len(atlas)))
    binary.extend(atlas)
    doc['images'].append(dict(bufferView=len(doc['bufferViews']) - 1, mimeType='image/png'))
    doc['textures'].append(dict(source=len(doc['images']) - 1, sampler=0))
    doc['materials'][primitive['material']]['pbrMetallicRoughness']['baseColorTexture']['index'] = len(doc['textures']) - 1
    primitive['attributes'] = io.add_skin_attributes(doc, binary, positions, io.smooth_normals(positions, faces), joints, skin)
    primitive['attributes']['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    primitive['indices'] = io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125)
    doc['meshes'][0]['name'] = 'glove_rogue_right'
    fit.with_rig(doc, binary, 'gloves_rogue', 'tripo_glove_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'glove_rogue_right.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images'][:len(images)]]
    single = fit.validate(target)
    binary = bytearray(binary)
    append_wrap(doc, binary, sources['previous_gloves'])
    fit.with_rig(doc, binary, 'gloves_rogue', 'tripo_glove_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'gloves_rogue.glb'
    fit.write_glb(target, doc, binary)
    report = dict(status='Fitted and rigged candidate; animation and visual review required', date='2026-10-02',
        source=sources['source'], base=sources['base'], interfaces=sources['interfaces'], rig_id=sources['rig_id'],
        topology=dict(source_triangles=len(source_faces), fitted_right_glove_triangles=len(faces), wrist_and_finger_openings=6,
            skin_conforming_hand_triangles=len(shell_faces), preserved_source_cuff_triangles=len(cuff_faces),
            method='Canonical hand surface offset 3mm and clipped at wrist/five proximal finger planes; source cuff/buckle retained, original texture baked into dorsal/palmar charts via inverse hand landmarks and outermost source rays',
            texture_atlas_dimensions=[4096, 2048], hand_chart_dimensions=[1024, 1024], original_embedded_texture_retained=True),
        alignment=alignment, finger_regions=regions,
        corrections=[dict(date='2026-10-02', region='Right index/thumb finger web',
            problem='The thumb opening plane clipped skin-weighted palm faces on the index side of the thumb, exposing a triangular patch while gripping the sword',
            fix='Extend only the index-side thumb opening by up to 20mm; preserve the rest of the glove and all embedded textures',
            previous_combined_sha256='4b649c00f29c44986dd4dc35ae49d8790cd4b201deccf4fb3be06ce4b573e968')],
        rejected_methods=[dict(method='Warp complete source finger lining', reason='Source finger lining folded between fingers and produced sharp deformation; replace palm/finger topology while preserving cuff and texture', initial_maximum_edge_stretch_ratio=16.003)],
        right_glove=single, combined_gloves=fit.validate(target), left_wrap=dict(source=sources['previous_gloves'], preserved_geometry_uv_material_and_weights=True))
    (ROOT / 'doc/assets/modular-rogue-tripo-glove-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['combined_gloves'], indent=2))


if __name__ == '__main__':
    main()
