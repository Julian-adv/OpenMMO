import hashlib
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy.spatial import cKDTree
from scipy.spatial.transform import Rotation
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'assets/monsters/thief_king/tripo_v1/source.glb'
DONOR = ROOT / 'assets/modular_human_male_01/fitted/base.glb'
OUT = ROOT / 'assets/monsters/thief_king/rig_v1/thief_king.glb'
REPORT = ROOT / 'doc/assets/thief-king-rig-v1.json'

spec = importlib.util.spec_from_file_location('barkan_rig', ROOT / 'tools/rig-barkan.py')
barkan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(barkan)


def landmarks(donor, worlds, ids):
    points = {
        'Hips': [0, 1.12, -.025], 'Spine': [0, 1.25, -.025],
        'Spine1': [0, 1.40, -.025], 'Spine2': [0, 1.56, -.025],
        'Neck': [0, 1.76, -.025], 'Head': [0, 1.87, -.015],
        'HeadTop_End': [0, 2.06, -.015],
    }
    left = {
        'UpLeg': [.135, 1.075, -.025], 'Leg': [.205, .605, -.055],
        'Foot': [.228, .14, -.065], 'ToeBase': [.23, .045, .105],
        'Toe_End': [.23, .03, .18], 'Shoulder': [.09, 1.73, -.025],
        'Arm': [.29, 1.69, -.025], 'ForeArm': [.54, 1.67, -.025],
        'Hand': [.815, 1.665, -.025],
    }
    names = {donor['nodes'][i]['name']: i for i in ids}
    for side, sign in [('Left', 1), ('Right', -1)]:
        for name, point in left.items():
            points[side + name] = np.array(point) * [sign, 1, 1]
        fingers = {
            'Index': (.935, 1.024, .037),
            'Middle': (.938, 1.044, .005),
            'Ring': (.932, 1.035, -.027),
            'Pinky': (.922, 1.014, -.058),
        }
        for finger, (start, end, z) in fingers.items():
            for i, t in enumerate([0, .34, .68, 1], 1):
                points[f'{side}Hand{finger}{i}'] = np.array([sign * (start + (end - start) * t), 1.646, z])
        for i, t in enumerate([0, .34, .68, 1], 1):
            points[f'{side}HandThumb{i}'] = np.array([sign * (.865 + .065 * t), 1.645, .025 + .066 * t])
    return {name: np.asarray(p, dtype=float) for name, p in points.items()}


def build():
    doc, data = read_glb(SOURCE)
    binary = bytearray(data)
    donor, donor_binary = read_glb(DONOR)
    worlds = barkan.world_matrices(donor)
    ids = donor['skins'][0]['joints']
    names = [donor['nodes'][i]['name'] for i in ids]
    points = landmarks(donor, worlds, ids)
    parents = {donor['nodes'][c]['name']: donor['nodes'][i]['name'] for i in ids
               for c in donor['nodes'][i].get('children', []) if c in ids}
    children = {name: [child for child in names if parents.get(child) == name] for name in names}
    new_world = {}
    for index, name in zip(ids, names):
        targets = children[name]
        primary = {'Hips': 'Spine', 'Spine2': 'Neck', 'LeftHand': 'LeftHandMiddle1', 'RightHand': 'RightHandMiddle1'}.get(name)
        child = primary or (targets[0] if targets else None)
        rotation = worlds[index][:3, :3]
        if child:
            old_child = ids[names.index(child)]
            rotation = barkan.rotate_between(worlds[old_child][:3, 3] - worlds[index][:3, 3], points[child] - points[name]) @ rotation
        matrix = np.eye(4)
        matrix[:3, :3] = rotation
        matrix[:3, 3] = points[name]
        new_world[name] = matrix

    primitive = doc['meshes'][0]['primitives'][0]
    source_points = accessor(doc, binary, primitive['attributes']['POSITION']).astype(float)
    factor = 2.1 / np.ptp(source_points[:, 1])
    feet = source_points[source_points[:, 1] < source_points[:, 1].min() + .025]
    offset = np.array([-(feet[:, 0].min() + feet[:, 0].max()) * factor / 2,
                       -source_points[:, 1].min() * factor,
                       -(feet[:, 2].min() + feet[:, 2].max()) * factor / 2])
    vertices = source_points * factor + offset
    primitive['attributes']['POSITION'] = add_accessor(doc, binary, vertices, 'VEC3')
    dense, distance = barkan.transfer_body_weights(donor, donor_binary, worlds, new_world, names, points, children, vertices)
    for side, sign in [('Left', 1), ('Right', -1)]:
        x = vertices[:, 0] * sign
        finger_names = ['Index', 'Middle', 'Ring', 'Pinky']
        chosen = np.abs(vertices[:, 2, None] - np.array([.037, .005, -.027, -.058])).argmin(axis=1)
        blend = np.clip((x - .905) / .035, 0, 1) * (vertices[:, 1] > 1.57) * (vertices[:, 2] < .065)
        for finger_index, finger in enumerate(finger_names):
            subset = np.flatnonzero((chosen == finger_index) & (blend > 0))
            start = abs(points[f'{side}Hand{finger}1'][0])
            end = abs(points[f'{side}Hand{finger}4'][0])
            t = np.clip((x[subset] - start) / (end - start) * 3, 0, 2)
            lower = np.floor(t).astype(int)
            part = t - lower
            dense[subset] *= (1 - blend[subset, None])
            for i in range(3):
                weight = np.where(lower == i, 1 - part, 0) + np.where(lower + 1 == i, part, 0)
                dense[subset, names.index(f'{side}Hand{finger}{i + 1}')] += blend[subset] * weight
        thumb = np.clip((vertices[:, 2] - .045) / .025, 0, 1) * np.clip((x - .84) / .04, 0, 1) * (vertices[:, 1] > 1.57)
        subset = np.flatnonzero(thumb > 0)
        t = np.clip((x[subset] - .865) / .065 * 3, 0, 2)
        lower = np.floor(t).astype(int)
        part = t - lower
        dense[subset] *= 1 - thumb[subset, None]
        for i in range(3):
            weight = np.where(lower == i, 1 - part, 0) + np.where(lower + 1 == i, part, 0)
            dense[subset, names.index(f'{side}HandThumb{i + 1}')] += thumb[subset] * weight
    for side in ['Left']:
        for finger in ['Thumb', 'Index', 'Middle', 'Ring', 'Pinky']:
            first = names.index(f'{side}Hand{finger}1')
            for i in [2, 3, 4]:
                joint = names.index(f'{side}Hand{finger}{i}')
                dense[:, first] += dense[:, joint]
                dense[:, joint] = 0
    hood = np.clip((vertices[:, 1] - 1.80) / .10, 0, 1) * (abs(vertices[:, 0]) < .20)
    hood = hood * hood * (3 - 2 * hood)
    dense *= 1 - hood[:, None]
    dense[:, names.index('Head')] += hood
    triangles = accessor(doc, binary, primitive['indices']).reshape(-1, 3)
    edges = np.concatenate([triangles[:, [0, 1]], triangles[:, [1, 2]], triangles[:, [2, 0]]])
    lengths = np.linalg.norm(vertices[edges[:, 0]] - vertices[edges[:, 1]], axis=1)
    strength = np.exp(-lengths / .035)
    rows = np.concatenate([edges[:, 0], edges[:, 1]])
    columns = np.concatenate([edges[:, 1], edges[:, 0]])
    adjacency = coo_matrix((np.tile(strength, 2), (rows, columns)), shape=(len(vertices), len(vertices))).tocsr()
    totals = np.asarray(adjacency.sum(axis=1)).reshape(-1, 1)
    pairs = cKDTree(vertices).query_pairs(1e-6, output_type='ndarray')
    links = coo_matrix((np.ones(len(pairs) * 2),
                        (np.concatenate([pairs[:, 0], pairs[:, 1]]), np.concatenate([pairs[:, 1], pairs[:, 0]]))),
                       shape=(len(vertices), len(vertices)))
    group_count, groups = connected_components(links, directed=False)
    group_sizes = np.bincount(groups)
    for _ in range(8):
        averaged = adjacency @ dense / np.maximum(totals, 1e-12)
        dense = np.where(totals > 0, .5 * dense + .5 * averaged, dense)
        group_weights = np.zeros((group_count, len(names)))
        np.add.at(group_weights, groups, dense)
        dense = group_weights[groups] / group_sizes[groups, None]
    assert np.isfinite(dense).all() and (dense.sum(axis=1) > 0).all()
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    weights /= weights.sum(axis=1, keepdims=True)
    joints[weights == 0] = 0
    for attr, values, component in [('JOINTS_0', joints, 5123), ('WEIGHTS_0', weights, 5126)]:
        i = add_accessor(doc, binary, values, 'VEC4', component)
        primitive['attributes'][attr] = i
        doc['bufferViews'][doc['accessors'][i]['bufferView']]['target'] = 34962
    doc['bufferViews'][doc['accessors'][primitive['attributes']['POSITION']]['bufferView']]['target'] = 34962
    doc['nodes'] = [{'name': 'ThiefKing', 'mesh': 0, 'skin': 0}]
    mapping = {name: i + 1 for i, name in enumerate(names)}
    for name in names:
        parent = parents.get(name)
        local = np.linalg.inv(new_world[parent]) @ new_world[name] if parent else new_world[name]
        node = {'name': name, 'translation': local[:3, 3].tolist(), 'rotation': Rotation.from_matrix(local[:3, :3]).as_quat().tolist()}
        if children[name]:
            node['children'] = [mapping[child] for child in children[name]]
        doc['nodes'].append(node)
    doc['nodes'].append({'name': 'Armature', 'children': [mapping['Hips']]})
    doc['scenes'] = [{'nodes': [0, len(doc['nodes']) - 1]}]
    inverse = np.asarray([np.linalg.inv(new_world[name]).T.reshape(16) for name in names])
    doc['skins'] = [{'joints': list(mapping.values()), 'inverseBindMatrices': add_accessor(doc, binary, inverse, 'MAT4'), 'skeleton': mapping['Hips']}]
    doc['buffers'][0]['byteLength'] = len(binary)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    write_glb(OUT, doc, binary)
    sources = [SOURCE, DONOR, ROOT / 'doc/images/monsters/thief-king-concept.png']
    report = {'date': '2026-11-01', 'height_m': 2.1, 'height_definition': 'bind-pose soles to hood',
              'units': 'metres', 'runtime_scale': 1, 'geometry_scale': factor, 'geometry_offset_m': offset.tolist(),
              'sources': [{'path': str(p.relative_to(ROOT)), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in sources],
              'method': 'Local fit of the modular male 65-joint Mixamo-compatible hierarchy and surface weight transfer from the proportion-fitted donor, with fitted finger chains and articulated right-finger weights, a smooth hood-to-head transition and topology-based weight smoothing and identical weights for coincident seam vertices.',
              'joints': len(names), 'triangles': doc['accessors'][primitive['indices']]['count'] // 3,
              'vertices': len(vertices), 'unweighted_vertices': int((weights.sum(axis=1) == 0).sum()),
              'max_weight_sum_error': float(np.abs(weights.astype(np.float32).sum(axis=1) - 1).max()),
              'max_transfer_distance_m': float(distance.max()), 'landmarks_m': {n: p.tolist() for n, p in points.items()},
              'rig_output': str(OUT.relative_to(ROOT)), 'game_output': 'client/public/models/monsters/thief_king.glb'}
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['height_m', 'joints', 'triangles', 'vertices', 'unweighted_vertices', 'max_weight_sum_error', 'max_transfer_distance_m']}))


if __name__ == '__main__':
    build()
