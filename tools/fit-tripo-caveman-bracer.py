"""Fit and mirror the Tripo bracer onto the unchanged modular forearms."""
import importlib.util
import json
from pathlib import Path

import numpy as np

from lib.glb import view_bytes
from outfits.rogue_layers import wrist_section

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/caveman_tripo_bracer_v1'
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io
WRIST_OFFSET = .008
LENGTH = .230
SKIN_CUT_FRACTION = .23


def fitted_points(points, faces):
    wrist, elbow = fit.BONES['LeftHand'], fit.BONES['LeftForeArm']
    axis = io.unit(elbow - wrist)
    across = io.unit(np.cross(axis, [0, 0, 1]))
    basis = np.array([across, np.cross(across, axis)])
    source_basis = np.array([[1, 0, 0], [0, 0, 1]])
    theta = np.arange(128) * 2 * np.pi / 128
    radial = np.column_stack([np.cos(theta), np.sin(theta)])
    source_rows = np.linspace(.14, .82, 30)
    source_centers, source_radii = [], []
    for height in source_rows:
        center, radius = wrist_section(points[faces], np.array([0, height, 0]),
            np.array([0, 1, 0]), source_basis, radial, outermost=False)
        source_centers.append(center)
        source_radii.append(radius)
    heights = WRIST_OFFSET + points[:, 1] / points[:, 1].max() * LENGTH
    rows = np.linspace(heights.min(), heights.max(), 48)
    vertices, triangles, _ = fit.body_surface(('hands', 'forearms'), 1)
    centers, radii = [], []
    for height in rows:
        center, radius = wrist_section(vertices[triangles], wrist + axis * height,
            axis, basis, radial)
        centers.append(center)
        radii.append(radius)
    source_center = np.column_stack([np.interp(points[:, 1], source_rows, np.array(source_centers)[:, j]) for j in range(2)])
    cross = points[:, [0, 2]] - source_center
    angles = np.arctan2(cross[:, 1], cross[:, 0])

    def sample(table, levels, coordinates):
        values = np.array([np.interp(angles, theta, row, period=2 * np.pi) for row in table])
        return np.array([np.interp(height, levels, values[:, i]) for i, height in enumerate(coordinates)])

    depth = (np.linalg.norm(cross, axis=1) - sample(source_radii, source_rows, points[:, 1])) * .22
    allowance = .004 + .0015 * np.logaddexp(0, depth / .0015)
    radius = sample(radii, rows, heights) + allowance
    center = np.column_stack([np.interp(heights, rows, np.array(centers)[:, j]) for j in range(2)])
    result = wrist + heights[:, None] * axis
    result += (center + radius[:, None] * np.column_stack([np.cos(angles), np.sin(angles)])) @ basis
    return result, dict(wrist=wrist.tolist(), axis=axis.tolist(), basis=basis.tolist(),
        axial_interval_m=[float(heights.min()), float(heights.max())], body_sections=len(rows), angular_samples=len(theta),
        minimum_radial_allowance_m=float(allowance.min()), maximum_radial_allowance_m=float(allowance.max()),
        upper_skin_overlap_m=float(heights.max() - np.linalg.norm(elbow - wrist) * (1 - SKIN_CUT_FRACTION)))


def main():
    source = OUTPUT / 'source.glb'
    base = ROOT / 'assets/modular_human_male_01/parts/fitted/base.glb'
    assert fit.digest(source) == '17ef57be917df0988f0e1f41ca19e759a9a305b3fc2135de5dc58a155a0b9ad0'
    assert fit.digest(base) == 'ae72eb53953dd86b716859a402700eab536863e245c5261acb2592e5ef87ea5b'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    points = io.accessor(doc, raw, primitive['attributes']['POSITION']).astype(float)
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    left, alignment = fitted_points(points, faces)
    doc['meshes'] = []
    binary = bytearray(raw)
    for side in ['Left', 'Right']:
        fitted, triangles = left.copy(), faces.copy()
        if side == 'Right':
            fitted[:, 0] *= -1
            triangles = triangles[:, ::-1]
        joints = np.tile([fit.NAMES.index(side + 'ForeArm'), 0, 0, 0], (len(fitted), 1)).astype(np.uint16)
        skin = np.tile([1., 0, 0, 0], (len(fitted), 1))
        attrs = io.add_skin_attributes(doc, binary, fitted, io.smooth_normals(fitted, triangles), joints.copy(), skin.copy())
        attrs['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
        doc['meshes'].append(dict(name='caveman_bracer_' + side.lower(), primitives=[dict(attributes=attrs,
            material=primitive['material'], indices=io.add_accessor(doc, binary, triangles.reshape(-1, 1), 'SCALAR', 5125))]))
    fit.with_rig(doc, binary, 'gloves_caveman', 'tripo_caveman_bracer_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'gloves_caveman.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    for mesh in doc['meshes']:
        assert np.array_equal(uv, io.accessor(doc, binary, mesh['primitives'][0]['attributes']['TEXCOORD_0']))
    report = dict(date='2026-10-04', source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
        base=dict(path=str(base.relative_to(ROOT)), sha256=fit.digest(base)),
        interfaces=dict(path='assets/modular_human_male_01/parts/interfaces/v1/interfaces.json',
            sha256=fit.digest(ROOT / 'assets/modular_human_male_01/parts/interfaces/v1/interfaces.json')),
        method='Fit hollow bracer against actual forearm radial sections, retaining layered fur/ties and original topology/UV/texture. Mirror X and reverse winding. Both bracers follow their own ForeArm rigidly; wrists and fingers remain exposed and independently animated.',
        alignment=alignment, source_triangles=len(faces), skin_cut_fraction=SKIN_CUT_FRACTION,
        original_topology_uv_and_embedded_texture_preserved=True, canonical_body_unchanged=True, validation=fit.validate(target))
    (ROOT / 'doc/assets/modular-caveman-tripo-bracer-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
