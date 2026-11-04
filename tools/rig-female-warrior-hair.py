import copy
import hashlib
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/hair/tripo_warrior_v1/fitted_v1'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
spec = importlib.util.spec_from_file_location('io', ROOT / 'tools/fit-modular-plate.py')
io = importlib.util.module_from_spec(spec)
spec.loader.exec_module(io)
doc, binary = read_glb(OUT / 'hair-fitted-unskinned.glb')
binary = bytearray(binary)
base, base_binary = read_glb(BODY)
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
armature = len(doc['nodes'])
rig_id = 'human_female_01_local_v1'
doc['nodes'].append({'name': 'Armature', 'children': [mapping[i] for i in joint_ids if i not in children],
                     'extras': {'rig_id': rig_id}})
doc['scenes'][0]['nodes'].append(armature)
doc['scenes'][0]['extras'] = {'rig_id': rig_id}
inverse = io.accessor(base, base_binary, skin['inverseBindMatrices'])
doc['skins'] = [{'joints': [mapping[i] for i in joint_ids],
                 'skeleton': mapping[skin['skeleton']],
                 'inverseBindMatrices': io.add_accessor(doc, binary, inverse, 'MAT4')}]

def smooth(low, high, value):
    t = np.clip((value - low) / (high - low), 0, 1)
    return t * t * (3 - 2 * t)

counts = {}
body_primitive = base['meshes'][0]['primitives'][0]
body_attrs = body_primitive['attributes']
body_points = io.accessor(base, base_binary, body_attrs['POSITION'])
body_joints = io.accessor(base, base_binary, body_attrs['JOINTS_0']).astype(int)
body_weights = io.accessor(base, base_binary, body_attrs['WEIGHTS_0'])
body_dense = np.zeros((len(body_points), len(names)))
np.add.at(body_dense, (np.arange(len(body_points))[:, None], body_joints), body_weights)
allowed = [names.index(name) for name in ['Hips', 'Spine', 'Spine1', 'Spine2']]
eligible = (body_points[:, 1] > .99) & (body_points[:, 1] < 1.521) & (np.abs(body_points[:, 0]) < .18)
body_dense[:, [i for i in range(len(names)) if i not in allowed]] = 0
eligible &= body_dense.sum(1) > .5
donor_points = body_points[eligible]
donor_weights = body_dense[eligible]
donor_weights /= donor_weights.sum(1, keepdims=True)
tree = cKDTree(donor_points)
for index in part_nodes:
    node = doc['nodes'][index]
    assert not any(key in node for key in ['matrix', 'translation', 'rotation', 'scale'])
    node.update(skin=0, extras={'rig_id': rig_id, 'part_id': 'hair_female_warrior'})
    for primitive in doc['meshes'][node['mesh']]['primitives']:
        attrs = primitive['attributes']
        points = io.accessor(doc, binary, attrs['POSITION'])
        distances, nearest = tree.query(points, k=256)
        factors = np.exp(-(distances ** 2 - distances[:, :1] ** 2) / .072 ** 2)
        factors /= factors.sum(1, keepdims=True)
        dense = (donor_weights[nearest] * factors[:, :, None]).sum(1)
        local_distances, local_nearest = tree.query(points, k=32)
        local_factors = np.exp(-(local_distances ** 2 - local_distances[:, :1] ** 2) / .025 ** 2)
        local_factors /= local_factors.sum(1, keepdims=True)
        local_dense = (donor_weights[local_nearest] * local_factors[:, :, None]).sum(1)
        chest = ((1 - smooth(1.44, 1.55, points[:, 1]))
                 * smooth(.065, .10, np.abs(points[:, 0])) * smooth(.02, .05, points[:, 2]))
        dense = dense * (1 - chest[:, None]) + local_dense * chest[:, None]
        head = smooth(1.35, 1.611, points[:, 1]) * (1 - chest)
        neck = (1 - head) * .25 * smooth(1.404, 1.548, points[:, 1])
        dense *= (1 - head[:, None] - neck[:, None])
        dense[:, names.index('Head')] += head
        dense[:, names.index('Neck')] += neck
        joints = np.argsort(-dense, axis=1)[:, :4]
        weights = np.take_along_axis(dense, joints, axis=1)
        weights /= weights.sum(1, keepdims=True)
        joints[weights == 0] = 0
        attrs['JOINTS_0'] = io.add_accessor(doc, binary, joints, 'VEC4', 5123)
        attrs['WEIGHTS_0'] = io.add_accessor(doc, binary, weights, 'VEC4')
        for key in ['JOINTS_0', 'WEIGHTS_0']:
            doc['bufferViews'][doc['accessors'][attrs[key]]['bufferView']]['target'] = 34962
        counts[node['name']] = {'vertices': len(points), 'rigid_head_vertices': int((head == 1).sum()),
                                'max_weight_sum_error': float(abs(weights.sum(1) - 1).max())}
doc['buffers'][0]['byteLength'] = len(binary)
target = OUT / 'hair_female_warrior.glb'
write_glb(target, doc, binary)
report_path = ROOT / 'doc/assets/modular-female-warrior-hair-fitting-v1.json'
report = json.loads(report_path.read_text())
report['rigging'] = {'rig_id': rig_id, 'bones': len(names), 'method': 'Transfer averaged female torso weights, use a more local transfer for front hair at the chest, and blend to rigid Head above the neck; exact female bind matrices and bone order.',
                     'parts': counts, 'physics': False, 'output': str(target.relative_to(ROOT)),
                     'sha256': hashlib.sha256(target.read_bytes()).hexdigest()}
report_path.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report['rigging']))
