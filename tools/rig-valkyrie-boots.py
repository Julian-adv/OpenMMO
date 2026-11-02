import copy
import hashlib
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb
from outfits.female_boot_pose import heel_pose
from outfits.valkyrie_heel_patch import patches

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/items/valkyrie_boots/fitted_v1'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
spec = importlib.util.spec_from_file_location('io', ROOT / 'tools/fit-modular-plate.py')
io = importlib.util.module_from_spec(spec)
spec.loader.exec_module(io)
source = OUT / 'boots-fitted-unskinned.glb'
assert hashlib.sha256(source.read_bytes()).hexdigest() == '74e135e94a67b10692f7354b570c59399a864429c87f146d401238c0faabba4a'
doc, binary = read_glb(source)
binary = bytearray(binary)
base, base_binary = read_glb(BODY)
heel_matrices, pose_config = heel_pose(base, base_binary, degrees=36)
skin = base['skins'][0]
joint_ids = skin['joints']
names = [base['nodes'][index]['name'] for index in joint_ids]
mapping = {index: len(doc['nodes']) + i for i, index in enumerate(joint_ids)}
part_nodes = list(doc['scenes'][0]['nodes'])
for index in joint_ids:
    bone = copy.deepcopy(base['nodes'][index])
    if 'children' in bone:
        bone['children'] = [mapping[i] for i in bone['children'] if i in mapping]
    doc['nodes'].append(bone)
children = {child for index in joint_ids for child in base['nodes'][index].get('children', [])}
rig_id = 'human_female_01_local_v1'
doc['nodes'].append({'name': 'Armature', 'children': [mapping[i] for i in joint_ids if i not in children], 'extras': {'rig_id': rig_id}})
doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
doc['scenes'][0]['extras'] = {'rig_id': rig_id, 'heel_pose': pose_config}
inverse = io.accessor(base, base_binary, skin['inverseBindMatrices'])
doc['skins'] = [{'joints': [mapping[i] for i in joint_ids], 'skeleton': mapping[skin['skeleton']], 'inverseBindMatrices': io.add_accessor(doc, binary, inverse, 'MAT4')}]
body_attrs = base['meshes'][0]['primitives'][0]['attributes']
body_points = io.accessor(base, base_binary, body_attrs['POSITION'])
body_faces = io.accessor(base, base_binary, base['meshes'][0]['primitives'][0]['indices']).reshape(-1, 3)
def smooth(low, high, value):
    t = np.clip((value - low) / (high - low), 0, 1)
    return t * t * (3 - 2 * t)

def fit_knee(points):
    outside = np.abs(points[:, 0])
    height = points[:, 1]
    weight = (smooth(.075, .105, outside) * smooth(.455, .485, height)
              * (1 - smooth(.515, .635, height)))
    center_x = np.interp(height, [.46, .52, .58, .64], [.20, .17, .148, .145]) * np.sign(points[:, 0])
    center_z = np.interp(height, [.46, .52, .58, .64], [-.065, -.059, -.035, -.027])
    fitted = points.copy()
    fitted[:, 0] -= (points[:, 0] - center_x) * .46 * weight
    fitted[:, 2] -= (points[:, 2] - center_z) * .46 * weight
    return fitted

normals = io.accessor(base, base_binary, body_attrs['NORMAL'])
points = body_points.astype(float)
fitted = fit_knee(points)
distance = fitted[:, 1] - .515 - np.maximum(0, np.abs(fitted[:, 0]) - .17) * .1
hidden = np.flatnonzero((distance[body_faces] < 0).all(axis=1)).tolist()
crossing = np.flatnonzero((distance[body_faces].min(axis=1) < 0)
                          & (distance[body_faces].max(axis=1) >= 0)).tolist()
indices = np.flatnonzero(np.linalg.norm(fitted - points, axis=1) > 1e-7)
steps = np.eye(3) * 1e-5
jacobian = np.stack([(fit_knee(points[indices] + step) - fit_knee(points[indices] - step)) / 2e-5
                     for step in steps], axis=2)
fitted_normals = np.linalg.solve(jacobian.transpose(0, 2, 1), normals[indices, :, None])[:, :, 0]
fitted_normals /= np.linalg.norm(fitted_normals, axis=1, keepdims=True)
adjustments = []
for index, normal in zip(indices, fitted_normals):
    adjustments.append({'vertex': int(index), 'position': fitted[index].tolist(), 'normal': normal.tolist()})
mask = {'mesh_index': 0, 'primitive_index': 0, 'primitive_triangles': len(body_faces),
        'hide_triangle_indices': hidden, 'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
        'vertex_adjustments': adjustments, 'clip_below_y': {'height': .515, 'outer_start': .17,
                                                              'outer_slope': .1, 'triangle_indices': crossing},
        'cutoff_m': .515,
        'description': 'Hide the body below the knees and fit the outer knee skin inside the boots.'}
(OUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')
body_joints = io.accessor(base, base_binary, body_attrs['JOINTS_0']).astype(int)
body_weights = io.accessor(base, base_binary, body_attrs['WEIGHTS_0'])
posed_body = np.einsum('nkij,nj,nk->ni', heel_matrices[body_joints],
                       np.column_stack([body_points, np.ones(len(body_points))]), body_weights)[:, :3]
foot_shift = float(posed_body[body_points[:, 1] < .144, 1].min())
dense_body = np.zeros((len(body_points), len(names)))
np.add.at(dense_body, (np.arange(len(body_points))[:, None], body_joints), body_weights)
counts = {}
heel_material = len(doc['materials'])
doc['materials'].append({'name': 'valkyrie_heel_silver', 'doubleSided': True,
                         'pbrMetallicRoughness': {'baseColorFactor': [.62, .62, .65, 1],
                                                  'metallicFactor': .92, 'roughnessFactor': .24}})
for index in part_nodes:
    node = doc['nodes'][index]
    assert not any(key in node for key in ['matrix', 'translation', 'rotation', 'scale'])
    node.update(skin=0, extras={'rig_id': rig_id, 'part_id': 'boots_female_valkyrie'})
    mesh = doc['meshes'][node['mesh']]
    heel_primitives = []
    for primitive in mesh['primitives']:
        attrs = primitive['attributes']
        points = io.accessor(doc, binary, attrs['POSITION'])
        source_normals = io.accessor(doc, binary, attrs['NORMAL'])
        source_uv = io.accessor(doc, binary, attrs['TEXCOORD_0'])
        source_tangents = io.accessor(doc, binary, attrs['TANGENT'])
        weights_all = np.zeros((len(points), 4))
        joints_all = np.zeros((len(points), 4), dtype=np.uint16)
        for side, sign in [('Left', 1), ('Right', -1)]:
            selected = points[:, 0] * sign >= 0
            allowed = [names.index(side + suffix) for suffix in ['UpLeg', 'Leg', 'Foot', 'ToeBase', 'Toe_End']]
            donor_weights = dense_body[:, allowed]
            eligible = (body_points[:, 0] * sign > 0) & (body_points[:, 1] < 1.0) & (donor_weights.sum(1) > .5)
            donor = donor_weights[eligible]
            donor = donor / donor.sum(1, keepdims=True)
            distances, nearest = cKDTree(body_points[eligible]).query(points[selected], k=8)
            factors = 1 / np.maximum(distances, .002) ** 2
            factors /= factors.sum(1, keepdims=True)
            values = (donor[nearest] * factors[:, :, None]).sum(1)
            top = np.argsort(-values, axis=1)[:, :4]
            weights = np.take_along_axis(values, top, axis=1)
            weights /= weights.sum(1, keepdims=True)
            weights_all[selected] = weights
            joints_all[selected] = np.array(allowed)[top]
        attrs['JOINTS_0'] = io.add_accessor(doc, binary, joints_all, 'VEC4', 5123)
        attrs['WEIGHTS_0'] = io.add_accessor(doc, binary, weights_all, 'VEC4')
        deformation = np.einsum('nk,nkij->nij', weights_all, heel_matrices[joints_all])
        fitted_points = points.copy()
        fitted_points[:, 0] += np.sign(points[:, 0]) * .015
        fitted_points[:, 1] += foot_shift
        bind_points = np.linalg.solve(deformation, np.column_stack([fitted_points, np.ones(len(points))])[..., None])[:, :3, 0]
        attrs['POSITION'] = io.add_accessor(doc, binary, bind_points, 'VEC3')
        if 'NORMAL' in attrs:
            normals = np.einsum('nji,nj->ni', deformation[:, :3, :3], source_normals)
            normals /= np.maximum(np.linalg.norm(normals, axis=1, keepdims=True), 1e-12)
            attrs['NORMAL'] = io.add_accessor(doc, binary, normals, 'VEC3')
        if 'TANGENT' in attrs:
            tangents = source_tangents.copy()
            direction = np.linalg.solve(deformation[:, :3, :3], tangents[:, :3, None])[:, :, 0]
            direction /= np.maximum(np.linalg.norm(direction, axis=1, keepdims=True), 1e-12)
            tangents[:, :3] = direction
            attrs['TANGENT'] = io.add_accessor(doc, binary, tangents, 'VEC4')
        for key in ['POSITION', 'NORMAL', 'TANGENT', 'JOINTS_0', 'WEIGHTS_0']:
            if key not in attrs:
                continue
            doc['bufferViews'][doc['accessors'][attrs[key]]['bufferView']]['target'] = 34962
        faces = io.accessor(doc, binary, primitive['indices']).reshape(-1, 3)
        centers = points[faces].mean(axis=1)
        triangles = points[faces]
        directions = np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0])
        directions /= np.maximum(np.linalg.norm(directions, axis=1, keepdims=True), 1e-12)
        heel = ((centers[:, 1] > .007) & (centers[:, 1] < .18)
                & (centers[:, 2] < -.07) & (directions[:, 2] < -.5) & (directions[:, 1] > -.25))
        heel[[1796, 1798, 1800, 1818, 1820, 1840, 1859, 1860, 1861, 1892, 2708, 2712]] = True
        assert heel.any() and (~heel).any()
        clipped = patches(points, faces)
        clipped_indices = np.array([item[0] for item in clipped])
        assert not heel[clipped_indices].any()
        keep = np.ones(len(faces), dtype=bool)
        keep[clipped_indices] = False
        red_faces = faces[keep & ~heel].tolist()
        metal_faces = faces[keep & heel].tolist()
        added = {key: [] for key in ['POSITION', 'NORMAL', 'TEXCOORD_0', 'TANGENT', 'JOINTS_0', 'WEIGHTS_0']}
        for face_index, outside, inside in clipped:
            face = faces[face_index]
            cache = {}

            def vertex(bary):
                original = int(np.argmax(bary))
                if bary[original] > 1 - 1e-9:
                    return int(face[original])
                key = tuple(np.round(bary, 10))
                if key in cache:
                    return cache[key]
                dense = np.zeros(len(names))
                for weight, source_vertex in zip(bary, face):
                    np.add.at(dense, joints_all[source_vertex], weights_all[source_vertex] * weight)
                joint = np.argsort(-dense)[:4]
                weight = dense[joint]
                weight /= weight.sum()
                transform = np.einsum('k,kij->ij', weight, heel_matrices[joint])
                fitted_point = bary @ fitted_points[face]
                position = np.linalg.solve(transform, np.r_[fitted_point, 1])[:3]
                normal = transform[:3, :3].T @ (bary @ source_normals[face])
                normal /= np.linalg.norm(normal)
                tangent = np.linalg.solve(transform[:3, :3], bary @ source_tangents[face, :3])
                tangent /= np.linalg.norm(tangent)
                tangent = np.r_[tangent, 1 if bary @ source_tangents[face, 3] >= 0 else -1]
                for key_name, value in [('POSITION', position), ('NORMAL', normal),
                                        ('TEXCOORD_0', bary @ source_uv[face]), ('TANGENT', tangent),
                                        ('JOINTS_0', joint), ('WEIGHTS_0', weight)]:
                    added[key_name].append(value)
                index = len(points) + len(added['POSITION']) - 1
                cache[key] = index
                return index

            def append_polygon(polygon, output):
                if len(polygon) < 3:
                    return
                indices = [vertex(bary) for bary in polygon]
                for i in range(1, len(indices) - 1):
                    if len({indices[0], indices[i], indices[i + 1]}) == 3:
                        output.append([indices[0], indices[i], indices[i + 1]])

            for polygon in outside:
                append_polygon(polygon, red_faces)
            append_polygon(inside, metal_faces)
        if added['POSITION']:
            arrays = {'POSITION': bind_points, 'NORMAL': normals, 'TEXCOORD_0': source_uv,
                      'TANGENT': tangents, 'JOINTS_0': joints_all, 'WEIGHTS_0': weights_all}
            for key, values in arrays.items():
                component = 5123 if key == 'JOINTS_0' else 5126
                attrs[key] = io.add_accessor(doc, binary, np.concatenate([values, added[key]]),
                                             'VEC2' if key == 'TEXCOORD_0' else
                                             'VEC3' if key in ['POSITION', 'NORMAL'] else 'VEC4', component)
                doc['bufferViews'][doc['accessors'][attrs[key]]['bufferView']]['target'] = 34962
        primitive['indices'] = io.add_accessor(doc, binary, np.array(red_faces).reshape(-1, 1), 'SCALAR', 5125)
        heel_primitives.append({'attributes': dict(attrs), 'indices': io.add_accessor(
            doc, binary, np.array(metal_faces).reshape(-1, 1), 'SCALAR', 5125), 'material': heel_material})
        counts[node['name']] = {'vertices': len(points) + len(added['POSITION']),
                                'max_weight_sum_error': float(abs(weights_all.sum(1) - 1).max())}
    mesh['primitives'].extend(heel_primitives)
doc['buffers'][0]['byteLength'] = len(binary)
target = OUT / 'boots_female_valkyrie.glb'
write_glb(target, doc, binary)
report_path = OUT / 'fitting.json'
report = json.loads(report_path.read_text())
report['status'] = 'Rigged preliminary fitting candidate with heel pose; skin occlusion, animation and runtime compatibility not validated.'
report['heel_pose'] = pose_config
report['heel_pose']['fitted_vertical_shift_m'] = foot_shift
report['lateral_shift_per_boot_m'] = .015
report['heel_material'] = {'material': 'valkyrie_heel_silver', 'metallic_factor': .92,
                           'roughness_factor': .24, 'geometry_changed': False}
report['rigging'] = {'rig_id': rig_id, 'bones': len(names), 'method': 'Nearest eight female body vertices on the same leg, restricted to leg and foot bones.', 'parts': counts, 'output': str(target.relative_to(ROOT)), 'sha256': hashlib.sha256(target.read_bytes()).hexdigest()}
report_path.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report['rigging']))
