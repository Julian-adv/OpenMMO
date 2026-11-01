import copy
import hashlib
import json
import os
from pathlib import Path

import numpy as np
import trimesh
from scipy.interpolate import PchipInterpolator
from scipy.ndimage import gaussian_filter, gaussian_filter1d

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.female_boot_pose import heel_pose, node_worlds
from outfits.mesh_components import welded_islands
from outfits.radial_sections import radii
from outfits.skinning import deform

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'assets/items/female_caveman_boots/tripo_v1'
SOURCE = DIRECTORY / 'source/model_url.glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
VERSION = os.environ.get('FEMALE_CAVEMAN_BOOT_FIT_VERSION', 'v3')
assert VERSION in {'v1', 'v2', 'v3'}
OUTPUT = DIRECTORY / f'fitted_{VERSION}'
OUTPUT.mkdir(exist_ok=True)
CLEARANCE = .0035 if VERSION in {'v2', 'v3'} else .005
CUFF_HEIGHT = .47
SEATED_CUFF_DROP = 0.0


def smooth(value):
    t = np.clip(value, 0, 1)
    return t*t*(3-2*t)


def sections(triangles, height):
    result = []
    for a, b in [(triangles[:, 0], triangles[:, 1]), (triangles[:, 1], triangles[:, 2]),
                 (triangles[:, 2], triangles[:, 0])]:
        selected = (a[:, 1] < height) != (b[:, 1] < height)
        a, b = a[selected], b[selected]
        result.extend(a+(b-a)*((height-a[:, 1])/(b[:, 1]-a[:, 1]))[:, None])
    return np.array(result)


def profiles(triangles, heights, smoothing):
    bounds = [sections(triangles, y) for y in heights]
    assert all(len(section) >= 3 for section in bounds)
    centers = np.array([(section.min(0)+section.max(0))/2 for section in bounds])
    extents = np.array([(section.max(0)-section.min(0))/2 for section in bounds])
    centers = gaussian_filter1d(centers, smoothing, axis=0)
    centers[:, 1] = heights
    extents = gaussian_filter1d(extents, smoothing, axis=0)
    return centers, extents


doc, raw = read_glb(SOURCE)
base, body_raw = read_glb(BODY)
primitive = doc['meshes'][0]['primitives'][0]
attrs = primitive['attributes']
original = accessor(doc, raw, attrs['POSITION']).astype(float)
source_normals = accessor(doc, raw, attrs['NORMAL']).astype(float)
faces = accessor(doc, raw, primitive['indices']).reshape(-1, 3)
first, welded, labels = welded_islands(original, faces)
main = max(np.unique(labels), key=lambda label: len(np.unique(welded[labels == label])))
triangles = original[faces]
valid = np.linalg.norm(np.cross(triangles[:, 1]-triangles[:, 0], triangles[:, 2]-triangles[:, 0]), axis=1) > 1e-12
removed = int((~valid).sum())
faces = faces[valid]
rotation = np.eye(3)
source = original @ rotation.T
cloth_faces = faces[(labels[faces] == main).all(1)]
source_heights = np.linspace(.30, .985, 160)
source_centers, source_extents = profiles(source[cloth_faces], source_heights, 4)
bp = base['meshes'][0]['primitives'][0]
ba = bp['attributes']
bv = accessor(base, body_raw, ba['POSITION'])
bf = accessor(base, body_raw, bp['indices']).reshape(-1, 3)
bj = accessor(base, body_raw, ba['JOINTS_0']).astype(int)
bw = accessor(base, body_raw, ba['WEIGHTS_0'])
_, _, body_labels = welded_islands(bv, bf)
skin = np.bincount(body_labels).argmax()
right = (body_labels == skin) & (bv[:, 0] < 0) & (bv[:, 0] > -.36) & (bv[:, 1] < .89)
body_faces = bf[right[bf].all(1)]
body_triangles = bv[body_faces]
heights = np.linspace(.145, .52, 160)
centers, extents = profiles(body_triangles, heights, 2)
heel_matrices, pose_config = heel_pose(base, body_raw)
posed_body = deform(bv, bj, bw, heel_matrices)
worlds = node_worlds(base)
nodes = {node.get('name'): i for i, node in enumerate(base['nodes'])}
ankle = worlds[nodes['RightFoot'], :3, 3]
heading = worlds[nodes['RightToeBase'], :3, 3]-ankle
heading[1] = 0
heading /= np.linalg.norm(heading)
lateral = np.cross([0, 1, 0], heading)
scale = .48
anchor = np.array([np.interp(.30, source_heights, source_centers[:, k]) for k in range(3)])
anchor[1] = .30
ground = ankle[1]-scale*anchor[1]
height_curve = PchipInterpolator([0, .16, .30, .42, .70, 1.0],
                                [ground, ankle[1]-.14*scale, ankle[1], .17, .33, CUFF_HEIGHT])
allowance_heights = np.linspace(.18, .465, 120)
allowance = np.ones((len(allowance_heights), 96))


def fit(points, seated=False):
    y = points[:, 1]
    height = height_curve(y)
    if seated:
        height -= SEATED_CUFF_DROP*smooth((y-.16)/.34)
    width_scale = scale*(.88 if VERSION == 'v3' else 1)
    length_scale = scale*(.90 if VERSION == 'v3' else 1)
    foot = ankle+(points[:, 0]-anchor[0])[:, None]*width_scale*lateral
    foot += ((points[:, 2]-anchor[2])*length_scale-(.004 if VERSION == 'v3' else 0))[:, None]*heading
    foot[:, 1] = height
    center = np.column_stack([np.interp(height, heights, centers[:, k]) for k in range(3)])
    center[:, 1] = height
    shaft = center.copy()
    for axis in [0, 2]:
        offset = points[:, axis]-np.interp(y, source_heights, source_centers[:, axis])
        radius = np.interp(height, heights, extents[:, axis])+CLEARANCE
        shaft[:, axis] += offset*radius/np.interp(y, source_heights, source_extents[:, axis])
    blend = smooth((y-.30)/.15)
    result = foot*(1-blend[:, None])+shaft*blend[:, None]
    radial = result[:, [0, 2]]-center[:, [0, 2]]
    theta = np.arctan2(-radial[:, 1], radial[:, 0]) % (2*np.pi)
    column = theta/(2*np.pi)*allowance.shape[1]
    lower = np.floor(column).astype(int) % allowance.shape[1]
    upper = (lower+1) % allowance.shape[1]
    fractions = column-np.floor(column)
    factors = np.array([np.interp(h, allowance_heights, allowance[:, lo])*(1-t)
                       + np.interp(h, allowance_heights, allowance[:, hi])*t
                       for h, lo, hi, t in zip(height, lower, upper, fractions)])
    factors = 1+(factors-1)*smooth((height-.15)/.08)
    factors += (.0075 if VERSION in {'v2', 'v3'} else .016)*smooth((height-.40)/.05)/np.maximum(np.linalg.norm(radial, axis=1), .001)
    result[:, [0, 2]] = center[:, [0, 2]]+radial*factors[:, None]
    if VERSION in {'v2', 'v3'}:
        radial = result[:, [0, 2]] - center[:, [0, 2]]
        bump = np.zeros(len(result))
        for location in [[-.1836, .4504, -.0104], [-.1847, .4536, -.118],
                         [-.2127, .4506, -.0151], [-.14, .4536, -.0882], [-.1565, .4536, -.1104]]:
            bump = np.maximum(bump, .008*np.exp(-np.sum(((result-location)/[.018, .016, .018])**2, axis=1)/2))
        result[:, [0, 2]] += radial/np.maximum(np.linalg.norm(radial, axis=1)[:, None], .001)*bump[:, None]
    return result


for _ in range(4):
    fitted = fit(source)
    shell = fitted[cloth_faces]
    factors = []
    for height in allowance_heights:
        center = np.array([np.interp(height, heights, centers[:, k]) for k in range(3)])
        skin_radius = radii(body_triangles, center, np.array([0., 1., 0.]), samples=96)
        shell_radius = radii(shell, center, np.array([0., 1., 0.]), samples=96)
        good = np.isfinite(skin_radius) & np.isfinite(shell_radius)
        factor = np.ones(96)
        factor[good] = np.clip((skin_radius[good]+CLEARANCE)/shell_radius[good], 1, 1.4)
        factors.append(factor)
    allowance *= gaussian_filter(np.array(factors), [2, 2], mode=['nearest', 'wrap'])

fitted = fit(source)
names = [base['nodes'][i]['name'] for i in base['skins'][0]['joints']]


def skin_weights(points, side):
    direction = -1 if side == 'Right' else 1
    selected = (body_labels == skin) & (bv[:, 0]*direction > 0) & (np.abs(bv[:, 0]) < .36) & (bv[:, 1] < .89)
    local_faces = bf[selected[bf].all(1)]
    surface = trimesh.Trimesh(posed_body, local_faces, process=False)
    closest, _, contact = trimesh.proximity.closest_point(surface, points)
    bary = trimesh.triangles.points_to_barycentric(surface.triangles[contact], closest).clip(0, 1)
    bary /= bary.sum(1, keepdims=True)
    shares = np.zeros((len(points), len(names)))
    for corner in range(3):
        neighbor = local_faces[contact, corner]
        for influence in range(4):
            np.add.at(shares, (np.arange(len(points)), bj[neighbor, influence]),
                      bary[:, corner]*bw[neighbor, influence])
    allowed = [names.index(name) for name in ['Hips']+[side+suffix for suffix in ['UpLeg', 'Leg', 'Foot', 'ToeBase']]]
    shares[:, [i for i in range(len(names)) if i not in allowed]] = 0
    shares[:, names.index(side+'Foot')] += shares[:, names.index(side+'ToeBase')]
    shares[:, names.index(side+'ToeBase')] = 0
    shares /= shares.sum(1, keepdims=True)
    foot_blend = 1-smooth((points[:, 1]-.105)/.085)
    shares *= 1-foot_blend[:, None]
    shares[:, names.index(side+'Foot')] += foot_blend
    for label in np.unique(labels):
        if label != main:
            shares[labels == label] = shares[labels == label].mean(0)
    joints = np.argsort(-shares, axis=1)[:, :4]
    weights = np.take_along_axis(shares, joints, axis=1)
    weights /= weights.sum(1, keepdims=True)
    return joints, weights


left_region = (body_labels == skin) & (bv[:, 0] > 0) & (bv[:, 0] < .36) & (bv[:, 1] < .89)
left_triangles = bv[bf[left_region[bf].all(1)]]
left_centers, _ = profiles(left_triangles, heights, 2)
left_allowance = np.ones_like(allowance)


def fit_left(points):
    center = np.column_stack([np.interp(points[:, 1], heights, left_centers[:, k]) for k in [0, 2]])
    radial = points[:, [0, 2]]-center
    theta = np.arctan2(-radial[:, 1], radial[:, 0]) % (2*np.pi)
    column = theta/(2*np.pi)*96
    lower = np.floor(column).astype(int) % 96
    upper = (lower+1) % 96
    fractions = column-np.floor(column)
    factors = np.array([np.interp(h, allowance_heights, left_allowance[:, lo])*(1-t)
                       + np.interp(h, allowance_heights, left_allowance[:, hi])*t
                       for h, lo, hi, t in zip(points[:, 1], lower, upper, fractions)])
    factors = 1+(factors-1)*smooth((points[:, 1]-.15)/.08)
    result = points.copy()
    result[:, [0, 2]] = center+radial*factors[:, None]
    if VERSION in {'v2', 'v3'}:
        radial = result[:, [0, 2]] - center
        bump = np.zeros(len(result))
        for location in [[.1499, .4536, -.1186], [.1846, .4536, -.1276]]:
            bump = np.maximum(bump, .008*np.exp(-np.sum(((result-location)/[.018, .016, .018])**2, axis=1)/2))
        result[:, [0, 2]] += radial/np.maximum(np.linalg.norm(radial, axis=1)[:, None], .001)*bump[:, None]
    return result


mirrored = fitted.copy()
mirrored[:, 0] *= -1
for _ in range(4):
    shell = fit_left(mirrored)[cloth_faces]
    factors = []
    for height in allowance_heights:
        center = np.array([np.interp(height, heights, left_centers[:, k]) for k in range(3)])
        skin_radius = radii(left_triangles, center, np.array([0., 1., 0.]), samples=96)
        shell_radius = radii(shell, center, np.array([0., 1., 0.]), samples=96)
        good = np.isfinite(skin_radius) & np.isfinite(shell_radius)
        factor = np.ones(96)
        factor[good] = np.clip((skin_radius[good]+CLEARANCE)/shell_radius[good], 1, 1.4)
        factors.append(factor)
    left_allowance *= gaussian_filter(np.array(factors), [2, 2], mode=['nearest', 'wrap'])
def fitted_normals(seated):
    jacobian = np.stack([(fit((original+np.eye(3)[j]*1e-5) @ rotation.T, seated)
                         - fit((original-np.eye(3)[j]*1e-5) @ rotation.T, seated))/2e-5 for j in range(3)], axis=2)
    result = np.linalg.solve(jacobian.transpose(0, 2, 1), source_normals[..., None])[..., 0]
    return result/np.linalg.norm(result, axis=1, keepdims=True)


normals = fitted_normals(False)
seated_points, seated_normals = fit(source, True), fitted_normals(True)
assert all(np.isfinite(values).all() for values in [fitted, normals])
binary = bytearray(raw)
doc['meshes'], doc['nodes'] = [], []
morphs = []
for side in ['Right', 'Left']:
    points, normal, indices = fitted.copy(), normals.copy(), faces.copy()
    seat_points, seat_normal = seated_points.copy(), seated_normals.copy()
    if side == 'Left':
        indices = indices[:, ::-1]
        for positions, directions in [(points, normal), (seat_points, seat_normal)]:
            positions[:, 0] *= -1
            directions[:, 0] *= -1
            derivative = np.stack([(fit_left(positions+np.eye(3)[j]*1e-5)-fit_left(positions-np.eye(3)[j]*1e-5))/2e-5 for j in range(3)], axis=2)
            directions[:] = np.linalg.solve(derivative.transpose(0, 2, 1), directions[..., None])[..., 0]
            directions /= np.linalg.norm(directions, axis=1, keepdims=True)
            positions[:] = fit_left(positions)
    joint, weights = skin_weights(points, side)
    assert np.isfinite(weights).all()
    deformation = np.einsum('nk,nkij->nij', weights, heel_matrices[joint])
    points = np.linalg.solve(deformation, np.column_stack([points, np.ones(len(points))])[..., None])[:, :3, 0]
    normal = np.einsum('nji,nj->ni', deformation[:, :3, :3], normal)
    normal /= np.linalg.norm(normal, axis=1, keepdims=True)
    seat_points = np.linalg.solve(deformation, np.column_stack([seat_points, np.ones(len(seat_points))])[..., None])[:, :3, 0]
    seat_normal = np.einsum('nji,nj->ni', deformation[:, :3, :3], seat_normal)
    seat_normal /= np.linalg.norm(seat_normal, axis=1, keepdims=True)
    morphs.append({'POSITION': seat_points-points, 'NORMAL': seat_normal-normal})
    new_attrs = {'POSITION': add_accessor(doc, binary, points, 'VEC3'),
                 'NORMAL': add_accessor(doc, binary, normal, 'VEC3'),
                 'TEXCOORD_0': attrs['TEXCOORD_0'],
                 'JOINTS_0': add_accessor(doc, binary, joint, 'VEC4', 5123),
                 'WEIGHTS_0': add_accessor(doc, binary, weights, 'VEC4')}
    index = len(doc['meshes'])
    mesh_name = f'boots_female_caveman_{side.lower()}'
    doc['meshes'].append({'name': mesh_name, 'primitives': [{'attributes': new_attrs,
                         'material': primitive['material'],
                         'indices': add_accessor(doc, binary, indices.reshape(-1, 1), 'SCALAR', 5125)}]})
    doc['nodes'].append({'name': mesh_name, 'mesh': index, 'skin': 0,
                         'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'boots_female_caveman'}})
ids = base['skins'][0]['joints']
mapping = {index: len(doc['nodes'])+i for i, index in enumerate(ids)}
for index in ids:
    node = copy.deepcopy(base['nodes'][index])
    if 'children' in node:
        node['children'] = [mapping[i] for i in node['children'] if i in mapping]
    doc['nodes'].append(node)
children = {child for index in ids for child in base['nodes'][index].get('children', [])}
pose_config['body_sha256'] = hashlib.sha256(BODY.read_bytes()).hexdigest()
pose_config['seated_hand_lift_m'] = 0.0
doc['scenes'] = [{'nodes': [0, 1, len(doc['nodes'])],
                  'extras': {'rig_id': 'human_female_01_local_v1', 'heel_pose': pose_config}}]
doc['nodes'].append({'name': 'Armature', 'children': [mapping[i] for i in ids if i not in children]})
doc['skins'] = [{'joints': [mapping[i] for i in ids], 'skeleton': mapping[base['skins'][0]['skeleton']],
                 'inverseBindMatrices': add_accessor(doc, binary, accessor(base, body_raw,
                     base['skins'][0]['inverseBindMatrices']), 'MAT4')}]
def compact(doc, binary):
    used = {skin['inverseBindMatrices'] for skin in doc['skins']}
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            used.update(primitive['attributes'].values())
            used.add(primitive['indices'])
            for index in primitive['attributes'].values():
                doc['bufferViews'][doc['accessors'][index]['bufferView']]['target'] = 34962
            doc['bufferViews'][doc['accessors'][primitive['indices']]['bufferView']]['target'] = 34963
    remap = {old: new for new, old in enumerate(sorted(used))}
    doc['accessors'] = [doc['accessors'][i] for i in sorted(used)]
    for skin in doc['skins']:
        skin['inverseBindMatrices'] = remap[skin['inverseBindMatrices']]
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            primitive['attributes'] = {k: remap[v] for k, v in primitive['attributes'].items()}
            primitive['indices'] = remap[primitive['indices']]
    refs = doc['accessors'] + doc['images']
    used_views = sorted({ref['bufferView'] for ref in refs})
    remap = {old: new for new, old in enumerate(used_views)}
    packed = bytearray()
    views = []
    for index in used_views:
        view = copy.deepcopy(doc['bufferViews'][index])
        start = view.get('byteOffset', 0)
        packed.extend(b'\0' * (-len(packed) % 4))
        view['byteOffset'] = len(packed)
        packed.extend(binary[start:start+view['byteLength']])
        views.append(view)
    for ref in refs:
        ref['bufferView'] = remap[ref['bufferView']]
    doc['bufferViews'] = views
    doc['buffers'] = [{'byteLength': len(packed)}]
    return bytes(packed)


binary = compact(doc, binary)
binary = bytearray(binary)
for mesh, morph in zip(doc['meshes'], morphs):
    mesh['weights'] = [0]
    mesh['extras'] = {'targetNames': ['pose_sit_idle']}
    mesh['primitives'][0]['targets'] = [{name: add_accessor(doc, binary, values, 'VEC3') for name, values in morph.items()}]
doc['buffers'][0]['byteLength'] = len(binary)
write_glb(OUTPUT / 'boots_female_caveman.glb', doc, binary)
original_doc, _ = read_glb(SOURCE)
assert [view_bytes(doc, binary, image['bufferView']) for image in doc['images']] == [
    view_bytes(original_doc, raw, image['bufferView']) for image in original_doc['images']]
covered = (bv[:, 1] < CUFF_HEIGHT-SEATED_CUFF_DROP-.024) & (np.abs(bv[:, 0]) < .36)
hidden = np.flatnonzero(covered[bf].all(1)).tolist()
mask = {'mesh_index': 0, 'primitive_index': 0, 'primitive_triangles': len(bf),
        'hide_triangle_indices': hidden, 'source_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
        'description': 'Covered legs and feet, 24mm below the seated cuff. Restore exact original indices when unequipped.'}
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2)+'\n')
(OUTPUT / 'heel-pose.json').write_text(json.dumps(pose_config, indent=2)+'\n')
report = {'date': '2026-11-01', 'source': {'path': str(SOURCE.relative_to(ROOT)),
          'sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest()}, 'body': {'path': str(BODY.relative_to(ROOT)),
          'sha256': hashlib.sha256(BODY.read_bytes()).hexdigest()}, 'rig_id': 'human_female_01_local_v1',
          'triangles': len(faces)*2, 'removed_degenerate_faces_per_boot': removed,
          'cuff_height_m': CUFF_HEIGHT, 'clearance_target_m': CLEARANCE,
          'seated_cuff_drop_m': SEATED_CUFF_DROP, 'seated_cuff_morph': 'pose_sit_idle',
          'posed_floor_offset': float(-fitted[:, 1].min()), 'heel_pose': pose_config,
          'hidden_body_triangles': len(hidden), 'original_uv_and_texture_bytes_preserved': True,
          'method': 'Source heading correction, anatomical ankle/knee/thigh sections, radial clearance, mirrored left boot independently refitted to left skin with reversed winding, barycentric same-leg skin transfer and inverse skinning from shared heel pose into the exact 65-bone bind.',
          'weights': 'Foot and sole rigid below ankle; smooth transition to leg skin weights; Hips permitted near cuff; no opposite-leg or ToeBase influence. Upper stud uses one shared weight blend.',
          'status': 'Fitting candidate; runtime and visual verification pending'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report, indent=2))
