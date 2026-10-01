"""Fit the Tripo left wrist wrap and retain the approved right glove."""
import copy
import importlib.util
import json
from pathlib import Path

import numpy as np

from lib.glb import view_bytes
from outfits.rogue_layers import clip_garment, wrist_section

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/rogue_tripo_wrap_v1'
spec = importlib.util.spec_from_file_location('glove_fit', ROOT / 'tools/fit-tripo-glove.py')
glove = importlib.util.module_from_spec(spec)
spec.loader.exec_module(glove)
fit, io = glove.fit, glove.io


def fit_sections(points, source_triangles):
    wrist, elbow = fit.BONES['LeftHand'], fit.BONES['LeftForeArm']
    axis = io.unit(elbow - wrist)
    across = io.unit(np.cross(axis, [0, 0, 1]))
    basis = np.array([across, np.cross(axis, across)])
    source_basis = np.array([[1, 0, 0], [0, 0, 1]])
    theta = np.arange(96) * 2 * np.pi / 96
    radial = np.column_stack([np.cos(theta), np.sin(theta)])
    source_heights = np.linspace(.131, .75, 24)
    surface = fit.body_surface(('hands', 'forearms'), 1)
    centers, radii = [], []
    for height in source_heights:
        center, radius = wrist_section(source_triangles, np.array([0, height, 0]), np.array([0, 1, 0]), source_basis, radial, outermost=False)
        centers.append(center)
        radii.append(radius)
    centers, radii = np.array(centers), np.array(radii)
    target_heights = .012 + (points[:, 1] - .13) / (.999512 - .13) * .080
    target_sections = np.linspace(target_heights.min(), target_heights.max(), 32)
    body_centers, body_radii = [], []
    for height in target_sections:
        center, radius = wrist_section(surface[0][surface[1]], wrist + axis * height, axis, basis, radial, outermost=False)
        body_centers.append(center)
        body_radii.append(radius)
    body_centers, body_radii = np.array(body_centers), np.array(body_radii)
    center = np.column_stack([np.interp(points[:, 1], source_heights, centers[:, i]) for i in range(2)])
    cross = points @ source_basis.T - center
    angles = np.arctan2(cross[:, 1], cross[:, 0])

    def sample(table, rows, coordinates):
        values = np.array([np.interp(angles, theta, row, period=2 * np.pi) for row in table])
        return np.array([np.interp(height, rows, values[:, i]) for i, height in enumerate(coordinates)])

    ratio = np.linalg.norm(cross, axis=1) / sample(radii, source_heights, points[:, 1])
    radius = sample(body_radii, target_sections, target_heights) + .003 + .006 * np.logaddexp(0, (ratio - 1) * 6)
    center = np.column_stack([np.interp(target_heights, target_sections, body_centers[:, i]) for i in range(2)])
    result = wrist + target_heights[:, None] * axis
    result += (center + radius[:, None] * np.column_stack([np.cos(angles), np.sin(angles)])) @ basis
    return result, dict(wrist=wrist.tolist(), axis=axis.tolist(), axial_range_m=[float(target_heights.min()), float(target_heights.max())],
        section_count=32, angular_samples=96, minimum_radial_clearance_m=.003,
        method='Fit to actual canonical left forearm radial sections; preserve source fold variation with a positive smooth radial allowance and retain original UVs/texture')


def main():
    sources = json.loads((ROOT / 'doc/assets/modular-rogue-tripo-wrap-sources.json').read_text())
    right = ROOT / sources['right_glove']['path']
    for source in [sources['source'], sources['base'], sources['right_glove']]:
        assert fit.digest(ROOT / source['path']) == source['sha256']
    doc, raw = fit.read_glb(OUTPUT / 'source.glb')
    primitive = doc['meshes'][0]['primitives'][0]
    source = io.accessor(doc, raw, primitive['attributes']['POSITION'])
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    source_faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    radial = np.linalg.norm(source[:, [0, 2]] - [0, -.01], axis=1)
    distance = np.maximum(.13 - source[:, 1], np.minimum((source[:, 1] - .75) * 10, .22 - radial))
    points, faces, uv = clip_garment(source, source_faces, uv, distance)
    unique, inverse, welded, _ = glove.topology(points, faces)
    openings = glove.edge_loops(welded)
    assert len(openings) == 2, 'Wrap requires two through openings'
    positions, alignment = fit_sections(unique, source[source_faces])
    joints, skin = fit.transfer(positions, fit.body_surface(('hands', 'forearms'), 1), {'LeftForeArm', 'LeftHand', 'LeftHandThumb1', 'LeftHandThumb2'})
    points = positions[inverse]
    area = np.linalg.norm(np.cross(points[faces[:, 1]] - points[faces[:, 0]], points[faces[:, 2]] - points[faces[:, 0]]), axis=1) / 2
    assert area.min() > 1e-12
    texture = view_bytes(doc, raw, doc['images'][0]['bufferView'])
    binary = bytearray(raw)
    primitive['attributes'] = io.add_skin_attributes(doc, binary, points, io.smooth_normals(points, faces), joints[inverse], skin[inverse])
    primitive['attributes']['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    primitive['indices'] = io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125)
    doc['meshes'][0]['name'] = 'wrap_rogue_left'
    fit.with_rig(doc, binary, 'gloves_rogue', 'tripo_wrap_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'wrap_rogue_left.glb'
    fit.write_glb(target, doc, binary)
    assert texture == view_bytes(doc, binary, doc['images'][0]['bufferView'])
    single = fit.validate(target)
    doc, raw = fit.read_glb(right)
    original = copy.deepcopy(doc)
    binary = bytearray(raw)
    glove.append_wrap(doc, binary, dict(path=str(target.relative_to(ROOT))))
    fit.with_rig(doc, binary, 'gloves_rogue', 'tripo_wrap_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'gloves_rogue.glb'
    fit.write_glb(target, doc, binary)
    old, new = original['meshes'][0]['primitives'][0], doc['meshes'][0]['primitives'][0]
    for key, index in old['attributes'].items():
        assert np.array_equal(io.accessor(original, raw, index), io.accessor(doc, binary, new['attributes'][key]))
    assert np.array_equal(io.accessor(original, raw, old['indices']), io.accessor(doc, binary, new['indices']))
    assert original['materials'] == doc['materials'][:len(original['materials'])]
    for index, image in enumerate(original['images']):
        assert view_bytes(original, raw, image['bufferView']) == view_bytes(doc, binary, doc['images'][index]['bufferView'])
    assert fit.digest(ROOT / sources['base']['path']) == sources['base']['sha256']
    report = dict(date='2026-10-02', status='Fitted and rigged; animation and visual review required', source=sources['source'], base=sources['base'],
        rig_id=sources['rig_id'], alignment=alignment, topology=dict(source_triangles=len(source_faces), fitted_triangles=len(faces), through_openings=2,
            source_correction='Trim uneven bottom below source Y=0.13; remove the interior top cap while preserving the outer lip and cloth coils',
            opening_vertices=[len(loop) for loop in openings], original_texture_preserved=True, minimum_triangle_area_m2=float(area.min())),
        left_wrap=single, combined_gloves=fit.validate(target), preserved_right_glove=dict(path=str(right.relative_to(ROOT)), sha256=fit.digest(right),
            geometry_uv_skin_weights_material_and_texture_exact=True), canonical_body_unchanged=True)
    (ROOT / 'doc/assets/modular-rogue-tripo-wrap-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['combined_gloves'], indent=2))


if __name__ == '__main__':
    main()
