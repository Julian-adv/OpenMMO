"""Open, fit and mirror the priest ankle boot on the current male rig."""
from collections import Counter
import importlib.util
import json
from pathlib import Path

import numpy as np

from lib.glb import view_bytes
from outfits.rogue_layers import edge_loops

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/priest_tripo_boots_v1'


def helper(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


fit = helper('fit', 'tools/fit-modular-rogue.py')
surface = helper('surface', 'tools/fit-modular-barbarian.py')
boots = helper('boots', 'tools/fit-tripo-ranger-boots.py')
io = fit.io


def fit_points(raw, faces):
    result = raw * [.34, .36, .32] + [.1664, .003, .045]
    heights = np.linspace(.32, .69, 40)
    profiles = np.array([io.section_bounds(raw[faces], np.array([0, y, 0]),
        np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]])) for y in heights])
    centers, radii = profiles.mean(1), (profiles[:, 1] - profiles[:, 0]) / 2
    vertices, indices, _ = fit.body_surface(('feet', 'ankles', 'legs'), 1)
    skin_triangles = vertices[indices]
    for i in np.flatnonzero(result[:, 1] > .12):
        y = result[i, 1]
        center = np.array([np.interp(raw[i, 1], heights, centers[:, j]) for j in range(2)])
        radius = np.array([np.interp(raw[i, 1], heights, radii[:, j]) for j in range(2)])
        cross = (raw[i, [0, 2]] - center) / radius
        length = np.linalg.norm(cross)
        direction = cross / max(length, 1e-9)
        low, high = io.section_bounds(skin_triangles, np.array([0, y, 0]),
            np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]]))
        origin = (low + high) / 2
        skin = surface.ray_surface(skin_triangles, [origin[0], y, origin[1]],
                                   [direction[0], 0, direction[1]])
        assert skin is not None
        distance = max((skin + .014) * length, skin + .009)
        target = origin + direction * distance
        blend = io.smoothstep((y - .12) / .06)
        result[i, [0, 2]] = result[i, [0, 2]] * (1 - blend) + target * blend
    return result


def open_and_line(points, uv, faces):
    center = (abs(points[:, 0]) < .002) & (abs(points[:, 1] - .48828125) < 1e-7)
    center &= abs(points[:, 2] + .25756836) < 1e-7
    caps = np.any(center[faces], axis=1)
    assert caps.sum() == 27
    cap_uv = uv[center].mean(0)
    faces = faces[~caps]
    used, inverse = np.unique(faces, return_inverse=True)
    points, uv, faces = points[used], uv[used], inverse.reshape(-1, 3)
    fitted = fit_points(points, faces)
    _, weld = np.unique(np.round(fitted, 6), axis=0, return_inverse=True)
    loops = edge_loops(weld[faces])
    assert len(loops) == 1
    counts = Counter(tuple(sorted(edge)) for face in weld[faces]
                     for edge in zip(face, np.roll(face, -1)))
    boundary = [(a, b) for face in faces for a, b in zip(face, np.roll(face, -1))
                if counts[tuple(sorted((weld[a], weld[b])))] == 1]
    rim_indices = np.unique(boundary)
    rim = fitted[rim_indices]
    origin = rim[:, [0, 2]].mean(0)
    lower = fitted.copy()
    offset = lower[rim_indices][:, [0, 2]] - origin
    lower[np.ix_(rim_indices, [0, 2])] -= io.unit(offset) * .002
    lower[rim_indices, 1] -= .025
    lower_uv = uv * .85 + cap_uv * .15
    start = len(fitted)
    lining = [[b, a, a + start] for a, b in boundary]
    lining += [[b, a + start, b + start] for a, b in boundary]
    points = np.vstack([fitted, lower])
    uv = np.vstack([uv, lower_uv])
    faces = np.vstack([faces, lining])
    used, inverse = np.unique(faces, return_inverse=True)
    return points[used], uv[used], inverse.reshape(-1, 3), rim, origin, np.flatnonzero(caps)


def cuff_profile(rim, origin, source):
    angles = np.mod(np.arctan2(rim[:, 2] - origin[1], rim[:, 0] - origin[0]), 2 * np.pi)
    order = np.argsort(angles)
    radius = np.linalg.norm(rim[:, [0, 2]] - origin, axis=1)
    profile = []
    for angle in np.linspace(0, 2 * np.pi, 128, endpoint=False):
        height = np.interp(angle, angles[order], rim[order, 1], period=2 * np.pi)
        distance = np.interp(angle, angles[order], radius[order], period=2 * np.pi)
        profile.append([round(float(height), 8), round(float(distance), 8)])
    data = dict(source=str(source.relative_to(ROOT)), sha256=fit.digest(source), origin=origin.tolist(),
                leftLeg=fit.NAMES.index('LeftLeg'), rightLeg=fit.NAMES.index('RightLeg'),
                overlap=.008, inset=.005, taper=.025, profile=profile)
    (ROOT / 'client/src/lib/data/priestBootCuff.json').write_text(json.dumps(data, indent=2) + '\n')
    return dict(minimum_rim_height_m=float(rim[:, 1].min()), maximum_rim_height_m=float(rim[:, 1].max()),
                overlap_m=data['overlap'], inset_m=data['inset'], profile='client/src/lib/data/priestBootCuff.json')


def main():
    source = OUTPUT / 'source.glb'
    base = io.PARTS / 'fitted/base.glb'
    assert fit.digest(source) == '6efea573830860d16c79273cea42d6565c96ad90dceb299b76d1ef05716364f5'
    assert fit.digest(base) == '0e629865af6c3feac3a4444e0cb2d5f2d3861bf2350d643cbff0f9858b83535a'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    points = io.accessor(doc, raw, primitive['attributes']['POSITION']).astype(float)
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    original_count = len(faces)
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    left, uv, faces, rim, origin, removed = open_and_line(points, uv, faces)
    binary = bytearray(raw)
    doc['meshes'] = []
    for side in ['Left', 'Right']:
        fitted, triangles = left.copy(), faces.copy()
        if side == 'Right':
            fitted[:, 0] *= -1
            triangles = triangles[:, ::-1]
        joints, weights = boots.weights(fitted, side)
        attributes = io.add_skin_attributes(doc, binary, fitted, io.smooth_normals(fitted, triangles), joints, weights)
        attributes['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
        doc['meshes'].append(dict(name='priest_boot_' + side.lower(), primitives=[dict(attributes=attributes,
            material=primitive['material'], indices=io.add_accessor(doc, binary, triangles.reshape(-1, 1), 'SCALAR', 5125))]))
    fit.with_rig(doc, binary, 'boots_priest', 'tripo_priest_boots_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'boots_priest.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    interfaces = io.PARTS / 'interfaces/v1/interfaces.json'
    data = json.loads(interfaces.read_text())
    current = fit.body_surface(('legs', 'ankles', 'feet'))
    errors = []
    for interface in data['interfaces']:
        if interface['name'].startswith('shoe_ankle_'):
            points = np.array(interface['contours']['center']['points'])
            nearest, _, _ = fit.nearest_surface(points, current)
            errors.extend(np.linalg.norm(points - nearest, axis=1))
    assert max(errors) < .003
    report = dict(date='2026-10-09', source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
        base=dict(path=str(base.relative_to(ROOT)), sha256=fit.digest(base)),
        interfaces=dict(path=str(interfaces.relative_to(ROOT)), sha256=fit.digest(interfaces), recorded_base_sha256=data['base_sha256'],
                        ankle_contour_maximum_distance_to_current_body_m=float(max(errors))),
        method='Remove recessed cap, fit actual body sections, retain UV and embedded JPEG, add 25mm open inner lining, mirror X with reversed winding and side-specific Leg/Foot/ToeBase weights.',
        initial_transform=dict(scale=[.34, .36, .32], translation=[.1664, .003, .045]),
        source_triangles=original_count, removed_cap_triangles=removed.tolist(), triangles_per_boot=len(faces),
        mirrored_triangles=len(faces) * 2, skin_cut_height_m=.235, cuff=cuff_profile(rim, origin, target),
        validation=fit.validate(target))
    (ROOT / 'doc/assets/modular-priest-tripo-boots-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
