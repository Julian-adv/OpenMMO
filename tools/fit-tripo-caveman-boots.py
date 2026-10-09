"""Fit a Tripo boot to the current body and mirror it onto the canonical rig."""
import importlib.util
import json
from pathlib import Path

import numpy as np

from lib.glb import view_bytes

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/caveman/tripo_boots_v1'
SHAFT_OUTER_CLEARANCE = .012
FUR_EXTRA_RADIUS = .027
CUFF_ROUND_START = .395
CUFF_ROUND_FULL = .435
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io
spec = importlib.util.spec_from_file_location('barbarian', ROOT / 'tools/fit-modular-barbarian.py')
barbarian = importlib.util.module_from_spec(spec)
spec.loader.exec_module(barbarian)


def fitted_points(raw, faces):
    result = raw * [.48, .48, .48] + [.1664, .004, .048]
    heights = np.linspace(.20, .96, 32)
    source_centers, source_radii = [], []
    for height in heights:
        low, high = io.section_bounds(raw[faces], np.array([0, height, 0]),
            np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]]))
        source_centers.append((low + high) / 2)
        source_radii.append((high - low) / 2)
    surface = fit.body_surface(('feet', 'ankles', 'legs'), 1)
    body_triangles = surface[0][surface[1]]
    for i, point in enumerate(raw):
        y = result[i, 1]
        if y < .115:
            continue
        center = np.array([np.interp(point[1], heights, np.array(source_centers)[:, j]) for j in range(2)])
        radius = np.array([np.interp(point[1], heights, np.array(source_radii)[:, j]) for j in range(2)])
        cross = (point[[0, 2]] - center) / radius
        distance = np.linalg.norm(cross)
        direction = cross / max(distance, 1e-9)
        low, high = io.section_bounds(body_triangles, np.array([0, y, 0]),
            np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]]))
        body_center = (low + high) / 2
        origin = np.array([body_center[0], y, body_center[1]])
        skin = barbarian.ray_surface(body_triangles, origin, [direction[0], 0, direction[1]])
        assert skin is not None
        fur = io.smoothstep((point[1] - .52) / .12)
        outer = skin + SHAFT_OUTER_CLEARANCE + FUR_EXTRA_RADIUS * fur
        radial = max(outer * distance, skin + .006)
        target = body_center + direction * radial
        blend = io.smoothstep((y - .115) / .060)
        result[i, [0, 2]] = result[i, [0, 2]] * (1 - blend) + target * blend
    return result


def weights(points, side):
    leg, foot, toe = [fit.NAMES.index(side + bone) for bone in ('Leg', 'Foot', 'ToeBase')]
    calf = io.smoothstep((points[:, 1] - .060) / .120)
    calf *= 1 - io.smoothstep((points[:, 2] - .035) / .050) * (1 - io.smoothstep((points[:, 1] - .115) / .055))
    toe_blend = .55 * io.smoothstep((points[:, 2] - .145) / .080)
    dense = np.column_stack([calf, (1 - calf) * (1 - toe_blend), (1 - calf) * toe_blend, np.zeros(len(points))])
    joints = np.tile([leg, foot, toe, 0], (len(points), 1)).astype(np.uint16)
    return joints, dense


def straighten_heel(points, faces):
    heights = np.linspace(.05, .24, 40)
    rear = []
    for height in heights:
        low, _ = io.section_bounds(points[faces], np.array([0, height, 0]),
            np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]]))
        rear.append(low[1])
    y = points[:, 1]
    current = np.interp(y, heights, rear)
    straight = np.interp(y, [heights[0], heights[-1]], [rear[0], rear[-1]])
    shift = np.maximum(current - straight, 0)
    blend = io.smoothstep((y - .05) / .020) * (1 - io.smoothstep((y - .205) / .035))
    blend *= io.smoothstep((-points[:, 2] - .030) / .040)
    result = points.copy()
    result[:, 2] -= shift * blend
    return result


def round_cuff(points, uv, faces):
    points, uv, faces = io.subdivide_edges(points, uv, faces,
        lambda a, b: min(a[1], b[1]) > CUFF_ROUND_START and np.linalg.norm(a - b) > .012,
        iterations=1)
    unique, inverse = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    edges = np.concatenate([faces[:, [0, 1]], faces[:, [1, 2]], faces[:, [2, 0]]])
    edges = np.unique(np.sort(inverse[edges], axis=1), axis=0)
    directed = np.concatenate([edges, edges[:, ::-1]])
    counts = np.bincount(directed[:, 0], minlength=len(unique))[:, None]
    blend = io.smoothstep((unique[:, 1] - CUFF_ROUND_START) / (CUFF_ROUND_FULL - CUFF_ROUND_START))[:, None]
    for _ in range(5):
        sums = np.zeros_like(unique)
        np.add.at(sums, directed[:, 0], unique[directed[:, 1]])
        unique += .45 * blend * (sums / np.maximum(counts, 1) - unique)
    result = points.copy()
    active = points[:, 1] > CUFF_ROUND_START
    result[active] = unique[inverse[active]]
    surface = fit.body_surface(('legs',), 1)
    triangles = surface[0][surface[1]]
    for i in np.flatnonzero(active):
        low, high = io.section_bounds(triangles, np.array([0, result[i, 1], 0]),
            np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]]))
        center = (low + high) / 2
        offset = result[i, [0, 2]] - center
        distance = np.linalg.norm(offset)
        direction = offset / distance
        skin = barbarian.ray_surface(triangles, [center[0], result[i, 1], center[1]], [direction[0], 0, direction[1]])
        assert skin is not None
        clearance = distance - skin
        rounded_clearance = .038 - .006 * np.logaddexp(0, (.038 - clearance) / .006)
        influence = io.smoothstep((points[i, 1] - CUFF_ROUND_START) / (CUFF_ROUND_FULL - CUFF_ROUND_START))
        distance += influence * (rounded_clearance - clearance)
        result[i, [0, 2]] = center + direction * max(distance, skin + .006)
    return result, uv, faces


def main():
    source = OUTPUT / 'source.glb'
    base = ROOT / 'assets/modular_human_male_01/fitted/base.glb'
    assert fit.digest(source) == '5c5077cf8a67480c2edba22fd0af0c7f3cbe19e124f9ed463983b71ef7934d46'
    assert fit.digest(base) == 'ae72eb53953dd86b716859a402700eab536863e245c5261acb2592e5ef87ea5b'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    points = io.accessor(doc, raw, primitive['attributes']['POSITION']).astype(float)
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    source_triangles = len(faces)
    images = [view_bytes(doc, raw, im['bufferView']) for im in doc['images']]
    left = fitted_points(points, faces)
    before_heel = left.copy()
    left = straighten_heel(left, faces)
    heel_fill = float(np.max(before_heel[:, 2] - left[:, 2]))
    fixed_heel = (before_heel[:, 1] <= .05) | (before_heel[:, 1] >= .24) | (before_heel[:, 2] >= -.03)
    assert np.array_equal(before_heel[fixed_heel], left[fixed_heel])
    left, uv, faces = round_cuff(left, uv, faces)
    doc['meshes'] = []
    binary = bytearray(raw)
    panels = []
    for side in ['Left', 'Right']:
        fitted, triangles = left.copy(), faces.copy()
        if side == 'Right':
            fitted[:, 0] *= -1
            triangles = triangles[:, ::-1]
        joints, skin = weights(fitted, side)
        attributes = io.add_skin_attributes(doc, binary, fitted, io.smooth_normals(fitted, triangles), joints.copy(), skin.copy())
        attributes['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
        name = 'caveman_boot_' + side.lower()
        doc['meshes'].append(dict(name=name, primitives=[dict(attributes=attributes, material=primitive['material'],
            indices=io.add_accessor(doc, binary, triangles.reshape(-1, 1), 'SCALAR', 5125))]))
        panels.append(dict(side=side, bounds_m=dict(minimum=fitted.min(0).tolist(), maximum=fitted.max(0).tolist()),
            bone_weights={side + bone:dict(minimum=float(skin[:, k].min()), maximum=float(skin[:, k].max())) for k, bone in enumerate(['Leg', 'Foot', 'ToeBase'])}))
    fit.with_rig(doc, binary, 'boots_caveman', 'tripo_caveman_boots_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'boots_caveman.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, im['bufferView']) for im in doc['images']]
    report = dict(date='2026-10-04', source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
        base=dict(path=str(base.relative_to(ROOT)), sha256=fit.digest(base)),
        interfaces=dict(path='assets/modular_human_male_01/interfaces/v1/interfaces.json', sha256=fit.digest(ROOT / 'assets/modular_human_male_01/interfaces/v1/interfaces.json')),
        method='Fit shaft and fur around actual body cross-sections; preserve original UV/texture and mirror with reversed triangle winding. Calf rigid to Leg, smooth ankle transition limited to shaft, instep rigid to Foot, modest smooth toe bend.',
        raw_initial_transform=dict(scale=[.48, .48, .48], translation=[.1664, .004, .048]),
        shaft_clearance_m=.006, shaft_outer_clearance_m=SHAFT_OUTER_CLEARANCE,
        fur_additional_radius_m=FUR_EXTRA_RADIUS, skin_cut_height_m=.43,
        rear_ankle_straightening=dict(start_height_m=.05, end_height_m=.24,
            maximum_fill_m=heel_fill, unchanged_sole_toe_and_cuff_vertices=True,
            method='Fill the rear ankle indentation toward the heel-to-shaft profile; rear-only smooth influence, no sole or toe changes'),
        cuff_rounding=dict(start_height_m=CUFF_ROUND_START, full_influence_height_m=CUFF_ROUND_FULL,
            maximum_subdivision_levels=1, smoothing_passes=5, preserved_opening_clearance_m=.006,
            maximum_lip_clearance_m=.038, lip_rounding_softness_m=.006),
        source_triangles=source_triangles, mirrored_triangles=len(faces) * 2, panels=panels, validation=fit.validate(target))
    (ROOT / 'doc/assets/modular-caveman-tripo-boots-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation']))


if __name__ == '__main__':
    main()
