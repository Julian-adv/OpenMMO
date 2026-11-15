"""Place and skin the split ceremony tuxedo on the modular male.

Parts move only as wholes; the sleeves also turn at the shoulder and elbow to the body's arm pose.

.venv/bin/python tools/fit-male-tuxedo.py
"""
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy import sparse
from scipy.sparse.linalg import factorized
from scipy.spatial import cKDTree
from scipy.spatial.transform import Rotation

from lib.glb import read_glb
from outfits.mannequin import kabsch
from outfits.rogue_layers import compact_weights

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'assets/items/male_ceremony_tuxedo/tripo_front_v1/segmented.glb'
OUTPUT = ROOT / 'assets/modular_human_male_01/tuxedo/tripo_v1'
REPORT = ROOT / 'doc/assets/modular-male-tuxedo-fitting-v1.json'
spec = importlib.util.spec_from_file_location('pants', ROOT / 'tools/fit-tripo-pants.py')
pants = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pants)
fit, io = pants.fit, pants.io
NAMES = fit.NAMES
SIDES = [('Left', 'left', 1), ('Right', 'right', -1)]
JOINT = io.BONES
# ICP pairs a point with the body only within this distance (m).
REACH = .05
# The sleeve turn fades in over this distance (m) past the joint.
JOINT_BLEND = (.03, .09)
# Length (m) of the sleeve end whose centre is aimed at the wrist.
CUFF_RING = .03


def record(path):
    return dict(path=str(path.relative_to(ROOT)), sha256=fit.digest(path))


def icp(moving, surface, turn, pivot=None):
    """Rigid ICP onto a body surface: a shift, or with `turn` a rotation too, about `pivot` when given."""
    rotation, shift = np.eye(3), np.zeros(3)
    centre = np.zeros(3) if pivot is None else pivot
    for _ in range(40):
        placed = (moving - centre) @ rotation.T + centre + shift
        nearest = fit.nearest_surface(placed, surface)[0]
        keep = np.linalg.norm(nearest - placed, axis=1) < REACH
        a, b = moving[keep] - centre, nearest[keep] - centre
        a0, b0 = (a.mean(0), b.mean(0)) if pivot is None else (np.zeros(3), np.zeros(3))
        if turn:
            rotation = kabsch(a - a0, b - b0)[0]
        shift = b0 - rotation @ a0
    placed = (moving - centre) @ rotation.T + centre + shift
    distance = np.linalg.norm(fit.nearest_surface(placed, surface)[0] - placed, axis=1)
    return rotation, shift, dict(shift_m=shift.round(4).tolist(),
                                 turn_degrees=round(float(np.degrees(Rotation.from_matrix(rotation).magnitude())), 2),
                                 median_distance_m=round(float(np.median(distance)), 4))


def adjacency(points, faces):
    edges = np.unique(np.sort(np.concatenate([faces[:, [0, 1]], faces[:, [1, 2]], faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    return sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(points),) * 2)


def smooth(values, graph, points, amount):
    rows, cols = graph.nonzero()
    edge = 1 / np.maximum(np.linalg.norm(points[rows] - points[cols], axis=1), .002) ** 2
    weighted = sparse.csr_matrix((edge, (rows, cols)), shape=graph.shape)
    laplacian = sparse.diags(np.asarray(weighted.sum(1)).ravel()) - weighted
    return factorized((sparse.eye(len(points)) * 10 + amount * laplacian).tocsc())(values * 10)


def align_sleeves(points, graph):
    """Turn each sleeve about the shoulder, then its lower half about the elbow, onto the body's arm."""
    torso = cKDTree(fit.body_surface(('torso',))[0])
    report = {}
    for side, key, sign in SIDES:
        arm = fit.body_surface(('upper_arms', 'forearms'), sign)
        near_arm = (cKDTree(arm[0]).query(points)[0] < torso.query(points)[0]) & (points[:, 0] * sign > 0)
        sleeve = np.clip(smooth(near_arm.astype(float), graph, points, .01), 0, 1)
        report[key] = {}
        for pivot, end, surface in [('Arm', 'ForeArm', arm), ('ForeArm', 'Hand', fit.body_surface(('forearms',), sign))]:
            centre = JOINT[side + pivot]
            axis = io.unit(JOINT[side + end] - centre)
            along = (points - centre) @ axis
            amount = sleeve * io.smoothstep((along - JOINT_BLEND[0]) / (JOINT_BLEND[1] - JOINT_BLEND[0]))
            rotation, _, fitted = icp(points[amount > .95], surface, True, pivot=centre)
            turn = Rotation.from_matrix(rotation)
            if pivot == 'ForeArm':
                # The sleeve fits the thick forearm loosely, so its end still sat 1.5 cm off the wrist: aim it there.
                end_ring = (amount > .95) & (along > along[amount > .95].max() - CUFF_RING)
                aim = turn.apply(points[end_ring] - centre).mean(0)
                correction = Rotation.from_rotvec(np.cross(io.unit(aim), axis))
                fitted['cuff_aim_degrees'] = round(float(np.degrees(correction.magnitude())), 2)
                turn = correction * turn
            turned = Rotation.from_rotvec(amount[:, None] * turn.as_rotvec()).apply(points - centre) + centre
            points = np.where(amount[:, None] > 0, turned, points)
            report[key][pivot] = fitted
    return points, report


def weights_from(points, graph, regions, allowed, amount=.003):
    dense = fit.nearest_surface(points, fit.body_surface(regions), candidates=96)[2]
    dense[:, [i for i, name in enumerate(NAMES) if not allowed(name)]] = 0
    for side, _, sign in SIDES:
        # Nothing on one side of the body follows the other side's limb.
        other = points[:, 0] * sign < -.02
        dense[np.ix_(other, [i for i, name in enumerate(NAMES) if name.startswith(side)])] = 0
    dense /= dense.sum(1, keepdims=True)
    dense = np.maximum(smooth(dense, graph, points, amount), 0)
    dense = np.maximum(dense - .007, 0)
    return compact_weights(dense / dense.sum(1, keepdims=True))


def top(points, graph):
    points, sleeves = align_sleeves(points, graph)
    trunk = ('Hips', 'Spine', 'Spine1', 'Spine2', 'Neck')
    limb = ('Shoulder', 'Arm', 'ForeArm')
    return points, weights_from(points, graph, ('torso', 'upper_arms', 'forearms', 'neck'),
                                lambda name: name in trunk or name.endswith(limb)), dict(sleeves=sleeves)


def trousers(points, graph, forward):
    # Shifting them whole pushed the waist out through the jacket front, so they lean from the hips instead.
    pivot = JOINT['Hips']
    angle = np.arctan2(forward, pivot[1] - points[:, 1].min())
    points = Rotation.from_rotvec([-angle, 0, 0]).apply(points - pivot) + pivot
    return points, pants.skin_weights(points, graph, np.ones(len(points), bool)), \
        dict(pivot='Hips', lean_degrees=round(float(np.degrees(angle)), 2), hem_forward_m=round(float(forward), 4))


def gloves(points, graph):
    report = {}
    for side, key, sign in SIDES:
        selected = points[:, 0] * sign > 0
        rotation, shift, report[key] = icp(points[selected], fit.body_surface(('hands',), sign), True)
        points[selected] = points[selected] @ rotation.T + shift
    return points, weights_from(points, graph, ('hands', 'forearms'),
                                lambda name: 'Hand' in name or name.endswith('ForeArm'), amount=.0005), report


def shoe_shifts(points):
    return {key: icp(points[points[:, 0] * sign > 0], fit.body_surface(('feet',), sign), False)[1:]
            for _, key, sign in SIDES}


def shoes(points, graph, shifts):
    for _, key, sign in SIDES:
        # Level only: the soles already stand on the floor.
        points[points[:, 0] * sign > 0] += shifts[key][0] * [1, 0, 1]
    return points, weights_from(points, graph, ('feet', 'ankles'),
                                lambda name: name.endswith(('Leg', 'Foot', 'ToeBase')) and 'Up' not in name), \
        {key: shifts[key][1] for key in shifts}


def main():
    source, raw = read_glb(SOURCE)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    nodes = {node['name']: source['meshes'][node['mesh']]['primitives'][0] for node in source['nodes']}

    def load(names):
        position, uv, faces = [], [], []
        count = 0
        for name in names:
            attributes = nodes[name]['attributes']
            p = io.accessor(source, raw, attributes['POSITION']).astype(float)
            faces.append(io.accessor(source, raw, nodes[name]['indices']).reshape(-1, 3).astype(int) + count)
            position.append(p)
            uv.append(io.accessor(source, raw, attributes['TEXCOORD_0']))
            count += len(p)
        return np.vstack(position), np.vstack(uv), np.vstack(faces)

    boots = load(['left_boots', 'right_boots'])
    shifts = shoe_shifts(boots[0])
    # The trousers follow the shoes so their hems stay on them.
    legs = np.mean([shifts[key][0][2] for key in shifts])
    jobs = {'top': (load(['top']), top), 'pants': (load(['pants']), lambda p, g: trousers(p, g, legs)),
            'gloves': (load(['left_glove', 'right_glove']), gloves), 'boots': (boots, lambda p, g: shoes(p, g, shifts))}
    report = dict(date='2026-11-15', source=record(SOURCE), base=record(ROOT / 'assets/modular_human_male_01/fitted/base.glb'),
                  rig_id='human_male_01_mixamo_candidate_v2', parts={})
    for name, ((original, uv, faces), place) in jobs.items():
        welded, inverse = np.unique(np.round(original, 5), axis=0, return_inverse=True)
        inverse = inverse.ravel()
        graph = adjacency(welded, inverse[faces])
        welded, (joints, weights), placement = place(welded.copy(), graph)
        positions = welded[inverse]
        doc = dict(asset=source['asset'], meshes=[dict(name=f'{name}_tuxedo', primitives=[dict(material=0)])],
                   materials=source['materials'], textures=source['textures'], images=source['images'],
                   samplers=source.get('samplers', []), bufferViews=source['bufferViews'],
                   accessors=[], buffers=[{}])
        doc = json.loads(json.dumps(doc))
        binary = bytearray(raw)
        primitive = doc['meshes'][0]['primitives'][0]
        normals = io.smooth_normals(positions, faces)
        primitive['attributes'] = io.add_skin_attributes(doc, binary, positions, normals, joints[inverse], weights[inverse])
        primitive['attributes']['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
        primitive['indices'] = io.add_accessor(doc, binary, faces.reshape(-1), 'SCALAR', 5125)
        fit.with_rig(doc, binary, f'{name}_tuxedo', 'tripo_tuxedo_v1')
        binary = io.compact(doc, binary)
        target = OUTPUT / f'{name}_tuxedo.glb'
        fit.write_glb(target, doc, binary)
        report['parts'][name] = dict(placement=placement, validation=fit.validate(target))
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({name: (part['placement'], part['validation']['triangles']) for name, part in report['parts'].items()}, indent=1))


if __name__ == '__main__':
    main()
