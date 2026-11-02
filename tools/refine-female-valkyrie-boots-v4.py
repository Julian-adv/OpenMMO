import hashlib
import json
from pathlib import Path

import numpy as np
from scipy.ndimage import gaussian_filter1d

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.female_boot_pose import heel_pose, node_worlds

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'assets/items/valkyrie_starter/tripo_boots_v1'
SOURCE = DIRECTORY / 'fitted_v1/boots_female_valkyrie.glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUTPUT = DIRECTORY / 'fitted_v4'
OUTPUT.mkdir(exist_ok=True)


def smooth(x):
    x = np.clip(x, 0, 1)
    return x * x * (3 - 2 * x)


def section_center(triangles, height):
    points = []
    for i, j in [(0, 1), (1, 2), (2, 0)]:
        a, b = triangles[:, i], triangles[:, j]
        select = (a[:, 1] < height) != (b[:, 1] < height)
        a, b = a[select], b[select]
        points.extend(a + (b-a) * ((height-a[:, 1])/(b[:, 1]-a[:, 1]))[:, None])
    points = np.array(points)
    return (points[:, [0, 2]].min(axis=0) + points[:, [0, 2]].max(axis=0)) / 2


doc, raw = read_glb(SOURCE)
base, body_raw = read_glb(BODY)
matrices, _ = heel_pose(base, body_raw)
a = base['meshes'][0]['primitives'][0]['attributes']
body_points = accessor(base, body_raw, a['POSITION'])
body_faces = accessor(base, body_raw, base['meshes'][0]['primitives'][0]['indices']).reshape(-1, 3)
worlds = node_worlds(base)
by_name = {node.get('name'): i for i, node in enumerate(base['nodes'])}
heights = np.linspace(.55, .72, 100)
centers, headings, ankles = {}, {}, {}
for side, direction in [('Right', -1), ('Left', 1)]:
    selected = (body_points[:, 0] * direction > 0) & (np.abs(body_points[:, 0]) < .36) & (body_points[:, 1] < .89)
    triangles = body_points[body_faces[selected[body_faces].all(1)]]
    centers[side] = gaussian_filter1d([section_center(triangles, h) for h in heights], 3, axis=0)
    ankles[side] = worlds[by_name[side+'Foot'], :3, 3]
    heading = worlds[by_name[side+'ToeBase'], :3, 3] - ankles[side]
    heading[1] = 0
    headings[side] = heading / np.linalg.norm(heading)


def refine(points, side):
    points = points.copy()
    y = points[:, 1]
    center = np.column_stack([np.interp(y, heights, centers[side][:, axis]) for axis in range(2)])
    cuff = .08 * smooth((y-.59)/.095)
    points[:, [0, 2]] = center + (points[:, [0, 2]]-center) * (1-cuff[:, None])
    projection = (points-ankles[side]) @ headings[side]
    toe = .020 * smooth((projection-.055)/.15) * (1-smooth((y-.12)/.08))
    points -= toe[:, None] * headings[side]
    return points


def transformed(points, normals, joints, weights, side):
    deformation = np.einsum('nk,nkij->nij', weights, matrices[joints])
    posed = np.einsum('nij,nj->ni', deformation, np.c_[points, np.ones(len(points))])[:, :3]
    pose_normals = np.linalg.solve(deformation[:, :3, :3].transpose(0, 2, 1), normals[..., None])[..., 0]
    eps = 1e-5
    jacobian = np.stack([(refine(posed + np.eye(3)[i]*eps, side)-refine(posed - np.eye(3)[i]*eps, side))/(2*eps) for i in range(3)], axis=2)
    refined_normals = np.linalg.solve(jacobian.transpose(0, 2, 1), pose_normals[..., None])[..., 0]
    refined = refine(posed, side)
    bind = np.linalg.solve(deformation, np.c_[refined, np.ones(len(refined))][..., None])[:, :3, 0]
    bind_normals = np.einsum('nji,nj->ni', deformation[:, :3, :3], refined_normals)
    bind_normals /= np.linalg.norm(bind_normals, axis=1, keepdims=True)
    return bind, bind_normals


binary = bytearray(raw)
for mesh in doc['meshes']:
    side = 'Right' if mesh['name'].endswith('_right') else 'Left'
    primitive = mesh['primitives'][0]
    attributes = primitive['attributes']
    positions = accessor(doc, raw, attributes['POSITION'])
    normals = accessor(doc, raw, attributes['NORMAL'])
    joints = accessor(doc, raw, attributes['JOINTS_0']).astype(int)
    weights = accessor(doc, raw, attributes['WEIGHTS_0'])
    new_positions, new_normals = transformed(positions, normals, joints, weights, side)
    target = primitive['targets'][0]
    seat_positions = positions + accessor(doc, raw, target['POSITION'])
    seat_normals = normals + accessor(doc, raw, target['NORMAL'])
    new_seat, new_seat_normals = transformed(seat_positions, seat_normals, joints, weights, side)
    attributes['POSITION'] = add_accessor(doc, binary, new_positions, 'VEC3')
    attributes['NORMAL'] = add_accessor(doc, binary, new_normals, 'VEC3')
    target['POSITION'] = add_accessor(doc, binary, new_seat - new_positions, 'VEC3')
    target['NORMAL'] = add_accessor(doc, binary, new_seat_normals - new_normals, 'VEC3')
    assert np.isfinite(new_positions).all() and np.isfinite(new_normals).all()
doc['buffers'][0]['byteLength'] = len(binary)
write_glb(OUTPUT / 'boots_female_valkyrie.glb', doc, binary)
(OUTPUT / 'heel-pose.json').write_bytes((SOURCE.parent / 'heel-pose.json').read_bytes())
mask = json.loads((SOURCE.parent / 'body-occlusion.json').read_text())
covered = (body_points[:, 1] < .670) & (np.abs(body_points[:, 0]) < .36)
mask['hide_triangle_indices'] = np.flatnonzero(covered[body_faces].all(1)).tolist()
mask['description'] = 'Legs and feet hidden up to 0.670 m only while boots are equipped; retain skin at the cuff.'
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')
report = {'source': str(SOURCE.relative_to(ROOT)), 'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
          'method': 'In the equipped heel pose, smoothly narrow the cuff in both horizontal axes by up to 8% around each leg center and shorten the toe by up to 20 mm, preserving original indices, UVs, skin weights, seated morph and heel pose.',
          'cuff_start_m': .59, 'cuff_full_m': .685, 'cuff_radial_scale': .92, 'hide_below_m': .670, 'toe_max_reduction_m': .020,
          'triangles': 2948, 'status': 'Fitting candidate; review idle and movement for clearance.'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
