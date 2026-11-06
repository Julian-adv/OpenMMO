import sys
import json
from pathlib import Path

import numpy as np
from scipy.spatial import cKDTree
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from lib.humanoid_rig import world_matrices, transfer_body_weights, fit_joint_worlds, prune_weights, add_skin_nodes, humanoid_spec

ROOT = Path(__file__).resolve().parents[1]
DONOR = ROOT / 'assets/modular_human_male_01/fitted/base.glb'


def landmarks(spec):
    points = {name: np.asarray(point, dtype=float) for name, point in spec['body_landmarks'].items()}
    for side, sign in [('Left', 1), ('Right', -1)]:
        for name, point in spec['side_landmarks'].items():
            points[side + name] = np.asarray(point, dtype=float) * [sign, 1, 1]
        for finger, endpoints in {**spec['finger_lanes'], 'Thumb': spec['thumb_line']}.items():
            start, end = np.asarray(endpoints, dtype=float)
            for i, fraction in enumerate([0, .34, .68, 1], 1):
                points[f'{side}Hand{finger}{i}'] = (start + (end - start) * fraction) * [sign, 1, 1]
    return points


def blend_chain(dense, subset, amount, t, names, prefix):
    lower = np.floor(t).astype(int)
    part = t - lower
    dense[subset] *= 1 - amount[subset, None]
    for i in range(3):
        weight = np.where(lower == i, 1 - part, 0) + np.where(lower + 1 == i, part, 0)
        dense[subset, names.index(f'{prefix}{i + 1}')] += amount[subset] * weight


def build(monster_id):
    spec = humanoid_spec(json.loads((ROOT / f'assets/monsters/{monster_id}/rig_spec.json').read_text()))
    source = ROOT / spec['source']
    out = ROOT / f'assets/monsters/{monster_id}/rig_v1/{monster_id}.glb'
    doc, data = read_glb(source)
    binary = bytearray(data)
    donor, donor_binary = read_glb(DONOR)
    worlds = world_matrices(donor)
    ids = donor['skins'][0]['joints']
    names = [donor['nodes'][i]['name'] for i in ids]
    points = landmarks(spec)
    parents = {donor['nodes'][c]['name']: donor['nodes'][i]['name'] for i in ids
               for c in donor['nodes'][i].get('children', []) if c in ids}
    children = {name: [child for child in names if parents.get(child) == name] for name in names}
    new_world = fit_joint_worlds(ids, names, worlds, points, children)

    primitive = doc['meshes'][0]['primitives'][0]
    source_points = accessor(doc, binary, primitive['attributes']['POSITION']).astype(float)
    factor = spec['height_m'] / np.ptp(source_points[:, 1])
    feet = source_points[source_points[:, 1] < source_points[:, 1].min() + .025]
    offset = np.array([-(feet[:, 0].min() + feet[:, 0].max()) * factor / 2,
                       -source_points[:, 1].min() * factor,
                       -(feet[:, 2].min() + feet[:, 2].max()) * factor / 2])
    vertices = source_points * factor + offset
    primitive['attributes']['POSITION'] = add_accessor(doc, binary, vertices, 'VEC3')
    dense, distance = transfer_body_weights(donor, donor_binary, worlds, new_world, names, points, children, vertices)
    rigid = spec.get('rigid_hands', {})
    for side, sign in [('Left', 1), ('Right', -1)]:
        if side in rigid:
            continue
        x = vertices[:, 0] * sign
        finger_names = ['Index', 'Middle', 'Ring', 'Pinky']
        chosen = np.abs(vertices[:, 2, None] - np.array([spec['finger_lanes'][f][0][2] for f in finger_names])).argmin(axis=1)
        finger_blend = spec['finger_blend']
        blend = np.clip((x - finger_blend['start_x']) / finger_blend['width_x'], 0, 1) * (vertices[:, 1] > finger_blend['min_y']) * (vertices[:, 2] < finger_blend['max_z'])
        for finger_index, finger in enumerate(finger_names):
            subset = np.flatnonzero((chosen == finger_index) & (blend > 0))
            start = abs(points[f'{side}Hand{finger}1'][0])
            end = abs(points[f'{side}Hand{finger}4'][0])
            t = np.clip((x[subset] - start) / (end - start) * 3, 0, 2)
            blend_chain(dense, subset, blend, t, names, f'{side}Hand{finger}')
        thumb_blend = spec['thumb_blend']
        thumb = np.clip((vertices[:, 2] - thumb_blend['start_z']) / thumb_blend['width_z'], 0, 1) * np.clip((x - thumb_blend['start_x']) / thumb_blend['width_x'], 0, 1) * (vertices[:, 1] > thumb_blend['min_y'])
        subset = np.flatnonzero(thumb > 0)
        t = np.clip((x[subset] - spec['thumb_line'][0][0]) / (spec['thumb_line'][1][0] - spec['thumb_line'][0][0]) * 3, 0, 2)
        blend_chain(dense, subset, thumb, t, names, f'{side}HandThumb')
    for finger in ['Thumb', 'Index', 'Middle', 'Ring', 'Pinky']:
        first = names.index(f'LeftHand{finger}1')
        for i in [2, 3, 4]:
            joint = names.index(f'LeftHand{finger}{i}')
            dense[:, first] += dense[:, joint]
            dense[:, joint] = 0
    hood_blend = spec['hood_blend']
    hood = np.clip((vertices[:, 1] - hood_blend['start_y']) / hood_blend['width_y'], 0, 1) * (abs(vertices[:, 0]) < hood_blend['max_abs_x'])
    hood = hood * hood * (3 - 2 * hood)
    dense *= 1 - hood[:, None]
    dense[:, names.index('Head')] += hood
    # Hands without finger rigging (a fused weapon, a mitten) move with the wrist.
    for side, start_x in rigid.items():
        hand = names.index(side + 'Hand')
        for i, name in enumerate(names):
            if name.startswith(side + 'Hand') and i != hand:
                dense[:, hand] += dense[:, i]
                dense[:, i] = 0
        beyond = vertices[:, 0] * (1 if side == 'Left' else -1) > start_x
        dense[beyond] = 0
        dense[beyond, hand] = 1
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
    joints, weights = prune_weights(dense)
    for attr, values, component in [('JOINTS_0', joints, 5123), ('WEIGHTS_0', weights, 5126)]:
        i = add_accessor(doc, binary, values, 'VEC4', component)
        primitive['attributes'][attr] = i
        doc['bufferViews'][doc['accessors'][i]['bufferView']]['target'] = 34962
    doc['bufferViews'][doc['accessors'][primitive['attributes']['POSITION']]['bufferView']]['target'] = 34962
    doc['nodes'] = [{'name': spec['node_name'], 'mesh': 0, 'skin': 0}]
    add_skin_nodes(doc, binary, names, parents, children, new_world)
    doc['buffers'][0]['byteLength'] = len(binary)
    out.parent.mkdir(parents=True, exist_ok=True)
    write_glb(out, doc, binary)
    print(json.dumps({'height_m': spec['height_m'], 'joints': len(names),
                      'triangles': doc['accessors'][primitive['indices']]['count'] // 3,
                      'vertices': len(vertices), 'unweighted_vertices': int((weights.sum(axis=1) == 0).sum()),
                      'max_weight_sum_error': float(np.abs(weights.astype(np.float32).sum(axis=1) - 1).max()),
                      'max_transfer_distance_m': float(distance.max())}))


if __name__ == '__main__':
    if len(sys.argv) != 2 or not sys.argv[1].replace('_', '').isalnum():
        raise SystemExit('Usage: .venv/bin/python tools/rig-humanoid-monster.py <id>')
    build(sys.argv[1])
