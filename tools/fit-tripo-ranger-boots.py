"""Fit and mirror the delivered ranger boot on the canonical male rig."""
import importlib.util
import json
from pathlib import Path

import numpy as np

from lib.glb import view_bytes

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/ranger_tripo_boots_v1'


def helper(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


fit = helper('fit', 'tools/fit-modular-rogue.py')
surface = helper('surface', 'tools/fit-modular-barbarian.py')
io = fit.io


def fit_shaft(raw, faces):
    result = raw * .48 + [.1664, .002, .048]
    heights = np.linspace(.20, .97, 50)
    profiles = np.array([io.section_bounds(raw[faces], np.array([0, y, 0]),
        np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]])) for y in heights])
    centers = profiles.mean(1)
    radii = (profiles[:, 1] - profiles[:, 0]) / 2
    vertices, indices, _ = fit.body_surface(('feet', 'ankles', 'legs'), 1)
    triangles = vertices[indices]
    for i in np.flatnonzero(result[:, 1] > .12):
        y = result[i, 1]
        center = np.array([np.interp(raw[i, 1], heights, centers[:, j]) for j in range(2)])
        radius = np.array([np.interp(raw[i, 1], heights, radii[:, j]) for j in range(2)])
        cross = (raw[i, [0, 2]] - center) / radius
        length = np.linalg.norm(cross)
        direction = cross / max(length, 1e-9)
        low, high = io.section_bounds(triangles, np.array([0, y, 0]),
            np.array([0, 1, 0]), np.array([[1, 0, 0], [0, 0, 1]]))
        origin = (low + high) / 2
        skin = surface.ray_surface(triangles, [origin[0], y, origin[1]], [direction[0], 0, direction[1]])
        assert skin is not None
        cuff = .006 * io.smoothstep((raw[i, 1] - .68) / .10)
        distance = max((skin + .012 + cuff) * length, skin + .006)
        target = origin + direction * distance
        blend = io.smoothstep((y - .12) / .065)
        result[i, [0, 2]] = result[i, [0, 2]] * (1 - blend) + target * blend
    return result


def weights(points, side):
    leg, foot, toe = [fit.NAMES.index(side + bone) for bone in ('Leg', 'Foot', 'ToeBase')]
    calf = io.smoothstep((points[:, 1] - .06) / .12)
    calf *= 1 - io.smoothstep((points[:, 2] - .035) / .05) * (1 - io.smoothstep((points[:, 1] - .115) / .055))
    toe_blend = .45 * io.smoothstep((points[:, 2] - .145) / .08)
    dense = np.column_stack([calf, (1 - calf) * (1 - toe_blend), (1 - calf) * toe_blend, np.zeros(len(points))])
    joints = np.tile([leg, foot, toe, 0], (len(points), 1)).astype(np.uint16)
    return joints, dense


def main():
    sources = json.loads((ROOT / 'doc/assets/modular-ranger-tripo-boots-sources.json').read_text())
    for entry in [sources['source'], sources['base'], sources['interfaces']]:
        assert fit.digest(ROOT / entry['path']) == entry['sha256']
    doc, raw = fit.read_glb(OUTPUT / 'source.glb')
    assert len(doc['meshes']) == 1 and len(doc['meshes'][0]['primitives']) == 1
    primitive = doc['meshes'][0]['primitives'][0]
    points = io.accessor(doc, raw, primitive['attributes']['POSITION']).astype(float)
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    left = fit_shaft(points, faces)
    binary = bytearray(raw)
    doc['meshes'] = []
    for side in ['Left', 'Right']:
        fitted, triangles = left.copy(), faces.copy()
        if side == 'Right':
            fitted[:, 0] *= -1
            triangles = triangles[:, ::-1]
        joints, skin = weights(fitted, side)
        attributes = io.add_skin_attributes(doc, binary, fitted, io.smooth_normals(fitted, triangles), joints, skin)
        attributes['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
        doc['meshes'].append(dict(name='ranger_boot_' + side.lower(), primitives=[dict(attributes=attributes,
            material=primitive['material'], indices=io.add_accessor(doc, binary, triangles.reshape(-1, 1), 'SCALAR', 5125))]))
    fit.with_rig(doc, binary, 'boots_ranger', 'tripo_ranger_boots_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'boots_ranger.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    for mesh in doc['meshes']:
        assert np.array_equal(uv, io.accessor(doc, binary, mesh['primitives'][0]['attributes']['TEXCOORD_0']))
    report = dict(date='2026-10-07', source=sources['source'], base=sources['base'], interfaces=sources['interfaces'],
        method='Fit shaft to actual left calf sections; preserve source topology, UV and texture; mirror X and reverse triangle winding; bind each boot to its own Leg/Foot/ToeBase.',
        initial_transform=dict(scale=.48, translation=[.1664, .002, .048]),
        shaft_outer_clearance_m=.012, shaft_minimum_vertex_clearance_m=.006, cuff_extra_clearance_m=.006,
        runtime_skin_cut_height_m=.43,
        runtime_ranger_pants_cuff_profile='client/src/lib/data/rangerBootCuff.json',
        preserved_source_topology_uv_and_embedded_texture=True,
        source_triangles=len(faces), mirrored_triangles=len(faces) * 2, validation=fit.validate(target))
    (ROOT / 'doc/assets/modular-ranger-tripo-boots-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation']))


if __name__ == '__main__':
    main()
