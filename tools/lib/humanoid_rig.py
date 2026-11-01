import numpy as np
from scipy.spatial import cKDTree
from scipy.spatial.transform import Rotation
from lib.glb_accessors import accessor, add_accessor

HUMANOID_DEFAULTS = {
    'body_landmarks': {
        'Hips': [0, 1.12, -.025], 'Spine': [0, 1.25, -.025],
        'Spine1': [0, 1.40, -.025], 'Spine2': [0, 1.56, -.025],
        'Neck': [0, 1.76, -.025], 'Head': [0, 1.87, -.015],
        'HeadTop_End': [0, 2.06, -.015],
    },
    'side_landmarks': {'ToeBase': [.23, .045, .105], 'Toe_End': [.23, .03, .18]},
    'hood_blend': {'start_y': 1.8, 'width_y': .1, 'max_abs_x': .2},
}


def humanoid_spec(spec):
    return {**HUMANOID_DEFAULTS, **spec,
            'body_landmarks': {**HUMANOID_DEFAULTS['body_landmarks'], **spec.get('body_landmarks', {})},
            'side_landmarks': {**HUMANOID_DEFAULTS['side_landmarks'], **spec.get('side_landmarks', {})},
            'hood_blend': {**HUMANOID_DEFAULTS['hood_blend'], **spec.get('hood_blend', {})}}


def world_matrices(doc):
    result = {}
    def visit(index, parent):
        node = doc['nodes'][index]
        if 'matrix' in node:
            matrix = np.asarray(node['matrix']).reshape(4, 4).T
        else:
            matrix = np.eye(4)
            matrix[:3, :3] = Rotation.from_quat(node.get('rotation', [0, 0, 0, 1])).as_matrix() @ np.diag(node.get('scale', [1, 1, 1]))
            matrix[:3, 3] = node.get('translation', [0, 0, 0])
        result[index] = parent @ matrix
        for child in node.get('children', []):
            visit(child, result[index])
    for index in doc['scenes'][0]['nodes']:
        visit(index, np.eye(4))
    return result


def rotate_between(before, after):
    a, b = before / np.linalg.norm(before), after / np.linalg.norm(after)
    cross = np.cross(a, b)
    sine = np.linalg.norm(cross)
    return Rotation.from_rotvec(cross / sine * np.arctan2(sine, a @ b)).as_matrix() if sine > 1e-9 else np.eye(3)


def transfer_body_weights(donor, binary, old_world, new_world, names, positions, children, points, child_filter=None):
    ids = donor['skins'][0]['joints']
    donor_names = [donor['nodes'][index]['name'] for index in ids]
    warped, surfaces = [], []
    transforms = []
    for index, name in zip(ids, donor_names):
        targets = [child for child in children[name] if child_filter is None or child_filter(child)]
        primary = {'Hips': 'Spine', 'Spine2': 'Neck', 'LeftHand': 'LeftHandMiddle1', 'RightHand': 'RightHandMiddle1'}.get(name)
        child = primary or (targets[0] if targets else None)
        rotation = new_world[name][:3, :3] @ old_world[index][:3, :3].T
        width = 1.5 if name in ['Hips', 'Spine', 'Spine1', 'Spine2', 'Neck', 'Head'] else 1.4
        stretch = np.eye(3) * width
        if child:
            old_child = ids[donor_names.index(child)]
            before = old_world[old_child][:3, 3] - old_world[index][:3, 3]
            after = positions[child] - positions[name]
            axis = before / np.linalg.norm(before)
            stretch += (np.linalg.norm(after) / np.linalg.norm(before) - width) * np.outer(axis, axis)
        matrix = np.eye(4)
        matrix[:3, :3] = rotation @ stretch
        matrix[:3, 3] = positions[name] - matrix[:3, :3] @ old_world[index][:3, 3]
        transforms.append(matrix)
    for node in donor['nodes']:
        if 'mesh' not in node or 'skin' not in node:
            continue
        for primitive in donor['meshes'][node['mesh']]['primitives']:
            attributes = primitive['attributes']
            vertices = accessor(donor, binary, attributes['POSITION'])
            joints = accessor(donor, binary, attributes['JOINTS_0'])
            weights = accessor(donor, binary, attributes['WEIGHTS_0'])
            used = np.unique(accessor(donor, binary, primitive['indices']))
            result = np.zeros_like(vertices, dtype=float)
            dense = np.zeros((len(vertices), len(names)))
            for k in range(4):
                for joint in np.unique(joints[:, k]):
                    mask = joints[:, k] == joint
                    matrix = transforms[int(joint)]
                    result[mask] += (vertices[mask] @ matrix[:3, :3].T + matrix[:3, 3]) * weights[mask, k, None]
                    dense[mask, names.index(donor_names[int(joint)])] += weights[mask, k]
            warped.append(result[used])
            surfaces.append(dense[used])
    vertices, weights = np.concatenate(warped), np.concatenate(surfaces)
    distance, nearest = cKDTree(vertices).query(points, k=8)
    blend = 1 / np.maximum(distance, .003) ** 3
    blend /= blend.sum(1, keepdims=True)
    dense = np.einsum('nk,nkj->nj', blend, weights[nearest])
    return dense, distance[:, 0]


def fit_joint_worlds(ids, names, worlds, points, children):
    result = {}
    for index, name in zip(ids, names):
        targets = children[name]
        primary = {'Hips': 'Spine', 'Spine2': 'Neck', 'LeftHand': 'LeftHandMiddle1', 'RightHand': 'RightHandMiddle1'}.get(name)
        child = primary or (targets[0] if targets else None)
        rotation = worlds[index][:3, :3]
        if child:
            old_child = ids[names.index(child)]
            rotation = rotate_between(worlds[old_child][:3, 3] - worlds[index][:3, 3], points[child] - points[name]) @ rotation
        matrix = np.eye(4)
        matrix[:3, :3] = rotation
        matrix[:3, 3] = points[name]
        result[name] = matrix
    return result


def prune_weights(dense):
    assert np.isfinite(dense).all() and (dense.sum(axis=1) > 0).all()
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    weights /= weights.sum(axis=1, keepdims=True)
    joints[weights == 0] = 0
    return joints, weights


def add_skin_nodes(doc, binary, names, parents, children, new_world):
    mapping = {name: len(doc['nodes']) + i for i, name in enumerate(names)}
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
