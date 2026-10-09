import copy
from collections import Counter
from functools import cache
from pathlib import Path

import numpy as np

from lib.glb import read_glb, write_glb

ROOT = Path(__file__).resolve().parents[1]
PARTS = ROOT / 'assets/modular_human_male_01'
BASE, BASE_BIN = read_glb(PARTS / 'fitted/base.glb')
DTYPES = {5121: '<u1', 5123: '<u2', 5125: '<u4', 5126: '<f4'}
WIDTHS = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}


def accessor(doc, binary, index):
    a = doc['accessors'][index]
    view = doc['bufferViews'][a['bufferView']]
    dtype = np.dtype(DTYPES[a['componentType']])
    width = WIDTHS[a['type']]
    return np.ndarray((a['count'], width), dtype=dtype, buffer=binary,
                      offset=view.get('byteOffset', 0) + a.get('byteOffset', 0),
                      strides=(view.get('byteStride', width * dtype.itemsize), dtype.itemsize)).copy()


def add_accessor(doc, binary, values, kind, component=5126):
    binary.extend(b'\0' * (-len(binary) % 4))
    values = np.asarray(values, dtype=DTYPES[component])
    view = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': values.nbytes})
    binary.extend(values.tobytes())
    a = {'bufferView': view, 'componentType': component, 'count': len(values), 'type': kind}
    if kind == 'VEC3':
        a.update(min=values.min(0).tolist(), max=values.max(0).tolist())
    doc['accessors'].append(a)
    return len(doc['accessors']) - 1


def add_skin_attributes(doc, binary, positions, normals, joints, weights):
    joints[weights == 0] = 0
    return {
        'POSITION': add_accessor(doc, binary, positions, 'VEC3'),
        'NORMAL': add_accessor(doc, binary, normals, 'VEC3'),
        'JOINTS_0': add_accessor(doc, binary, joints, 'VEC4', 5123),
        'WEIGHTS_0': add_accessor(doc, binary, weights, 'VEC4'),
    }


def unit(v):
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


def smoothstep(value):
    t = np.clip(value, 0, 1)
    return t * t * (3 - 2 * t)


def shin_center(y, side):
    knee, ankle = BONES[side + 'Leg'], BONES[side + 'Foot']
    t = (y - ankle[1]) / (knee[1] - ankle[1])
    return ankle + t[:, None] * (knee - ankle)


def bone_positions():
    matrices = {}
    def visit(index, parent):
        node = BASE['nodes'][index]
        x, y, z, w = node.get('rotation', [0, 0, 0, 1])
        matrix = np.eye(4)
        matrix[:3, :3] = np.array([
            [1-2*y*y-2*z*z, 2*x*y-2*z*w, 2*x*z+2*y*w],
            [2*x*y+2*z*w, 1-2*x*x-2*z*z, 2*y*z-2*x*w],
            [2*x*z-2*y*w, 2*y*z+2*x*w, 1-2*x*x-2*y*y],
        ]) @ np.diag(node.get('scale', [1, 1, 1]))
        matrix[:3, 3] = node.get('translation', [0, 0, 0])
        world = parent @ matrix
        matrices[node.get('name', '')] = world[:3, 3]
        for child in node.get('children', []):
            visit(child, world)
    for index in BASE['scenes'][0]['nodes']:
        visit(index, np.eye(4))
    return matrices


BONES = bone_positions()
BODY_POS, BODY_JOINTS, BODY_WEIGHTS = [], [], []
for mesh in BASE['meshes']:
    for primitive in mesh['primitives']:
        for dest, name in [(BODY_POS, 'POSITION'), (BODY_JOINTS, 'JOINTS_0'), (BODY_WEIGHTS, 'WEIGHTS_0')]:
            dest.append(accessor(BASE, BASE_BIN, primitive['attributes'][name]))
BODY_POS, BODY_JOINTS, BODY_WEIGHTS = map(np.concatenate, (BODY_POS, BODY_JOINTS, BODY_WEIGHTS))


def transfer_weights(positions):
    joints, weights = [], []
    for start in range(0, len(positions), 96):
        p = positions[start:start+96]
        distances = np.sum((p[:, None] - BODY_POS[None]) ** 2, axis=2)
        near = np.argpartition(distances, 3, axis=1)[:, :3]
        factors = 1 / np.maximum(np.take_along_axis(distances, near, axis=1), 1e-8)
        factors /= factors.sum(1, keepdims=True)
        for indices, blend in zip(near, factors):
            combined = np.zeros(len(BASE['skins'][0]['joints']))
            np.add.at(combined, BODY_JOINTS[indices].ravel(), (BODY_WEIGHTS[indices] * blend[:, None]).ravel())
            top = np.argsort(combined)[-4:][::-1]
            joints.append(top)
            weights.append(combined[top] / combined[top].sum())
    return np.array(joints, dtype='<u2'), np.array(weights, dtype='<f4')


def fit_plate_waist(positions):
    result = positions.copy()
    waist = smoothstep((result[:, 1] - .99) / .07)
    waist *= 1 - smoothstep((result[:, 1] - 1.15) / .09)
    waist *= 1 - smoothstep((abs(result[:, 0]) - .10) / .07)
    waist *= smoothstep((result[:, 2] - .015) / .045)
    result[:, 2] += .016 * waist
    return result


def section_bounds(triangles, center, axis, cross_axes):
    distances = (triangles - center) @ axis
    intersections = []
    for a, b in [(0, 1), (1, 2), (2, 0)]:
        crossing = distances[:, a] * distances[:, b] < 0
        t = distances[crossing, a] / (distances[crossing, a] - distances[crossing, b])
        intersections.append(triangles[crossing, a] + t[:, None] *
                             (triangles[crossing, b] - triangles[crossing, a]))
    points = (np.concatenate(intersections) - center) @ cross_axes.T
    if not len(points):
        raise ValueError('Missing arm cross-section')
    return np.array([points.min(0), points.max(0)])


@cache
def plate_arm_triangles():
    source, binary = read_glb(PARTS / 'knight/sources/top_plate.glb')
    primitive = source['meshes'][0]['primitives'][0]
    points = accessor(source, binary, primitive['attributes']['POSITION'])
    faces = accessor(source, binary, primitive['indices']).reshape(-1, 3)
    original = points[faces]
    body = []
    for node in BASE['nodes']:
        if node.get('extras', {}).get('region') not in ('upper_arms', 'forearms', 'hands'):
            continue
        for primitive in BASE['meshes'][node['mesh']]['primitives']:
            points = accessor(BASE, BASE_BIN, primitive['attributes']['POSITION'])
            faces = accessor(BASE, BASE_BIN, primitive['indices']).reshape(-1, 3)
            body.extend(points[faces])
    return original, np.array(body)


@cache
def plate_arm_sections(side, lo, hi):
    sign = 1 if side == 'Left' else -1
    original, body = plate_arm_triangles()
    body = body[body.mean(1)[:, 0] * sign > .12]
    a, b = (('Arm', 'ForeArm') if lo == .24 else ('ForeArm', 'Hand'))
    start, end = BONES[side + a], BONES[side + b]
    axis = unit(end - start)
    up = unit(np.array([-axis[1] * sign, abs(axis[0]), 0]))
    forward = unit(np.cross(axis, up)) * sign
    samples = np.linspace(max(lo, .30), min(hi, .94), 25)
    source_bounds, target_bounds = [], []
    for distance in samples:
        source_bounds.append(section_bounds(original, [sign * distance, 0, 0],
                                            np.array([sign, 0, 0]), np.eye(3)[1:]))
        center = start + (end - start) * ((distance - lo) / (hi - lo))
        bounds = section_bounds(body, center, axis, np.array([up, forward]))
        target_bounds.append(bounds + np.array([[-.008], [.008]]))
    return samples, np.array(source_bounds), np.array(target_bounds), up, forward


def interpolate_bounds(samples, bounds, distances):
    return [np.column_stack((np.interp(distances, samples, bounds[:, edge, 0]),
                             np.interp(distances, samples, bounds[:, edge, 1])))
            for edge in range(2)]


def top_fit(p):
    out = p * [.88, .92, .96] + [0, 1.31, -.025]
    out[:, 1] += np.clip((p[:, 1] - .21) / .10, 0, 1) * np.clip((.16 - abs(p[:, 0])) / .04, 0, 1) * .048
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        distance = p[:, 0] * sign
        shoulder = BONES[side + 'Arm']
        elbow = BONES[side + 'ForeArm']
        wrist = BONES[side + 'Hand']
        for lo, hi, a, b in [(.24, .59, shoulder, elbow), (.59, .95, elbow, wrist)]:
            t = (distance - lo) / (hi - lo)
            center = a + (b - a) * t[:, None]
            samples, source_bounds, target_bounds, up, forward = plate_arm_sections(side, lo, hi)
            source_low, source_high = interpolate_bounds(samples, source_bounds, distance)
            target_low, target_high = interpolate_bounds(samples, target_bounds, distance)
            cross = target_low + (p[:, 1:] - source_low) / (source_high - source_low) * (target_high - target_low)
            arm = center + cross[:, :1] * up + cross[:, 1:] * forward
            mask = (distance >= lo) & ((distance < hi) if hi != .95 else True)
            if lo == .24:
                shoulder_shape = center + (p[:, 1] - .205)[:, None] * up * .90
                shoulder_shape[:, 2] += (p[:, 2] + .024) * .94
                blend = smoothstep((distance - .30) / .14)
                arm = shoulder_shape * (1 - blend[:, None]) + arm * blend[:, None]
                blend = np.clip((distance - .20) / .09, 0, 1)
                arm = out * (1 - blend[:, None]) + arm * blend[:, None]
            out[mask] = arm[mask]
    return fit_plate_waist(out)


def subdivide_edges(positions, uv, indices, split_edge, iterations=2):
    points, texcoords = list(positions), list(uv)
    for _ in range(iterations):
        midpoints = {}
        for face in indices:
            for a, b in zip(face, np.roll(face, -1)):
                edge = tuple(sorted((a, b)))
                if edge in midpoints or not split_edge(points[a], points[b]):
                    continue
                midpoints[edge] = len(points)
                points.append((points[a] + points[b]) / 2)
                texcoords.append((texcoords[a] + texcoords[b]) / 2)
        triangles = []
        for face in indices:
            mids = [midpoints.get(tuple(sorted((a, b))))
                    for a, b in zip(face, np.roll(face, -1))]
            count = sum(mid is not None for mid in mids)
            if count == 0:
                triangles.append(face)
            elif count == 3:
                a, b, c = face
                ab, bc, ca = mids
                triangles.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]])
            else:
                start = next(i for i in range(3) if mids[i] is not None and
                             (count == 1 or mids[(i + 1) % 3] is not None))
                a, b, c = np.roll(face, -start)
                ab, bc, _ = np.roll(mids, -start)
                if count == 1:
                    triangles.extend([[a, ab, c], [ab, b, c]])
                else:
                    triangles.extend([[b, bc, ab], [a, ab, c], [ab, bc, c]])
        indices = np.array(triangles, dtype=int)
    return np.array(points), np.array(texcoords), indices


def subdivide_greave_hems(positions, uv, indices):
    return subdivide_edges(positions, uv, indices, lambda a, b:
        max(a[1], b[1]) < -.65 and np.linalg.norm(a - b) > .075)


def subdivide_helmet_cheeks(positions, uv, indices):
    def split_edge(a, b):
        p = np.array([a, b]) * [.174, .177, .150] + [0, 1.772, .003]
        if p[:, 1].min() >= 1.765 or p[:, 2].max() <= -.04:
            return False
        fitted = helmet_fit(np.array([a, b, (a + b) / 2]))
        return np.linalg.norm(fitted[2] - fitted[:2].mean(axis=0)) > .003
    return subdivide_edges(positions, uv, indices, split_edge, iterations=3)


def pants_fit(p):
    y = np.interp(p[:, 1], [-.951, -.10, .946], [.165, .553, 1.145])
    center_x = np.interp(p[:, 1], [-.951, -.10, .6, .946], [.33, .265, .215, .18])
    desired_x = np.interp(y, [.165, .553, 1.015, 1.145], [.163, .141, .096, .09])
    x = np.sign(p[:, 0]) * (desired_x + (abs(p[:, 0]) - center_x) * .50)
    blend = np.clip((p[:, 1] - .45) / .3, 0, 1)
    x = x * (1 - blend) + p[:, 0] * .55 * blend
    z_center = np.interp(p[:, 1], [-.951, -.25, .05, .5, .946], [-.10, -.10, .02, .02, .05])
    desired_z = np.interp(y, [.165, .553, 1.145], [-.035, -.025, -.012])
    result = np.column_stack([x, y, (p[:, 2] - z_center) * .51 + desired_z])
    hem = 1 - smoothstep((y - .175) / .16)
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        mask = p[:, 0] * sign > 0
        center = shin_center(y[mask], side)
        offset = result[mask] - center
        arch_x = offset[:, 0] - sign * .014
        front_angle = np.arctan2(arch_x / .055, offset[:, 2] / .06)
        angle = np.arctan2(offset[:, 0] / .055, offset[:, 2] / .06)
        front = np.maximum(np.cos(front_angle), 0)
        back = np.maximum(-np.cos(angle), 0)
        result[mask, 1] += hem[mask] * (-.055 + .070 * front ** 1.4 + .010 * back)
        center = shin_center(result[mask, 1], side)
        for axis in [0, 2]:
            result[mask, axis] = center[:, axis] + offset[:, axis] * (1 + .04 * hem[mask])
    return fit_greave_cuffs(fit_ankle_clearance(fit_plate_waist(result)))


def boots_fit(p):
    z = np.interp(p[:, 2], [-.63, .628], [-.104, .205])
    source_x = .47 + .20 * np.clip((p[:, 2] + .4) / 1.0, 0, 1)
    target_x = np.interp(z, [-.104, .205], [.174, .151])
    x = np.sign(p[:, 0]) * (target_x + (abs(p[:, 0]) - source_x) * .205)
    y = (p[:, 1] + .323282) * .355
    result = np.column_stack([x, y, z])
    blend = smoothstep((y - .085) / .09)
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        mask = p[:, 0] * sign > 0
        cuff = shin_center(y[mask], side)
        cuff[:, 0] += sign * (abs(p[mask, 0]) - .343) * .16
        cuff[:, 2] += (p[mask, 2] + .304) * .16
        result[mask] += (cuff - result[mask]) * blend[mask, None]
    return fit_ankle_clearance(result)


def fit_ankle_clearance(positions, width=.55, depth=.45, back=.022):
    result = positions.copy()
    blend = smoothstep((positions[:, 1] - .085) / .105)
    blend *= 1 - smoothstep((positions[:, 1] - .24) / .16)
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        mask = positions[:, 0] * sign > 0
        center = shin_center(positions[mask, 1], side)
        for axis, expansion in [(0, width), (2, depth)]:
            result[mask, axis] += (positions[mask, axis] - center[:, axis]) * expansion * blend[mask]
    result[:, 2] -= back * blend
    return result


@cache
def plate_boot_triangles():
    doc, binary = read_glb(PARTS / 'knight/sources/boots_plate.glb')
    triangles = []
    for primitive in doc['meshes'][0]['primitives']:
        positions = boots_fit(accessor(doc, binary, primitive['attributes']['POSITION']))
        indices = accessor(doc, binary, primitive['indices']).reshape(-1, 3)
        triangles.extend(positions[indices])
    return np.array(triangles)


def fit_greave_cuffs(positions):
    result = positions.copy()
    triangles = plate_boot_triangles()
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        mask = (positions[:, 0] * sign > 0) & (positions[:, 1] < .30)
        points = positions[mask]
        if not len(points):
            continue
        center = fit_ankle_clearance(shin_center(points[:, 1], side))
        offset = points - center
        offset[:, 1] = 0
        radius = np.linalg.norm(offset, axis=1)
        direction = offset / radius[:, None]
        origin = fit_ankle_clearance(shin_center(np.minimum(points[:, 1], .19), side))
        faces = triangles[triangles[:, :, 0].mean(axis=1) * sign > 0]
        edge1, edge2 = faces[:, 1] - faces[:, 0], faces[:, 2] - faces[:, 0]
        cross = np.cross(direction[:, None], edge2)
        determinant = np.sum(edge1 * cross, axis=2)
        reciprocal = np.zeros_like(determinant)
        np.divide(1, determinant, out=reciprocal, where=abs(determinant) > 1e-10)
        distance = origin[:, None] - faces[:, 0]
        u = np.sum(distance * cross, axis=2) * reciprocal
        q = np.cross(distance, edge1)
        v = np.sum(direction[:, None] * q, axis=2) * reciprocal
        t = np.sum(edge2 * q, axis=2) * reciprocal
        hit = (abs(determinant) > 1e-10) & (u >= 0) & (v >= 0) & (u + v <= 1) & (t > 0)
        surface = np.max(np.where(hit, t, 0), axis=1)
        if np.any(surface == 0):
            raise ValueError('Greave cuff ray missed the boot surface')
        blend = 1 - smoothstep((points[:, 1] - .20) / .10)
        shrink = np.minimum(surface + .004 - radius, 0) * blend * .6
        result[mask] += direction * shrink[:, None]
    return result


def align_crotch_weights(positions, joints, weights):
    names = [BASE['nodes'][i]['name'] for i in BASE['skins'][0]['joints']]
    dense = np.zeros((len(positions), len(names)))
    np.add.at(dense, (np.arange(len(positions))[:, None], joints), weights)
    x, y = abs(positions[:, 0]), positions[:, 1]
    blend = smoothstep((y - .78) / .06) * (1 - smoothstep((y - 1.04) / .08))
    blend *= 1 - smoothstep((x - .10) / .06)
    blend *= smoothstep((positions[:, 2] + .04) / .05)
    width = .015 + .10 * smoothstep((y - .83) / .12)
    left = smoothstep((positions[:, 0] + width) / (2 * width))
    hip = smoothstep((y - .88) / .15)
    dense *= 1 - blend[:, None]
    dense[:, names.index('Hips')] += blend * hip
    dense[:, names.index('LeftUpLeg')] += blend * (1 - hip) * left
    dense[:, names.index('RightUpLeg')] += blend * (1 - hip) * (1 - left)
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(axis=1, keepdims=True)


def align_ankle_weights(positions, joints, weights):
    names = [BASE['nodes'][i]['name'] for i in BASE['skins'][0]['joints']]
    dense = np.zeros((len(positions), len(names)))
    np.add.at(dense, (np.arange(len(positions))[:, None], joints), weights)
    y = positions[:, 1]
    blend = smoothstep((y - .115) / .04) * (1 - smoothstep((y - .32) / .06))
    foot = 1 - smoothstep((y - .10) / .085)
    dense *= 1 - blend[:, None]
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        mask = positions[:, 0] * sign > 0
        dense[mask, names.index(side + 'Leg')] += blend[mask] * (1 - foot[mask])
        dense[mask, names.index(side + 'Foot')] += blend[mask] * foot[mask]
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(axis=1, keepdims=True)


def helmet_fit(p):
    fitted = p * [.174, .177, .150] + [0, 1.772, .003]
    x, y, z = np.abs(fitted[:, 0]), fitted[:, 1], fitted[:, 2]
    ear = smoothstep((y - 1.69) / .05) * (1 - smoothstep((y - 1.81) / .04))
    ear *= smoothstep((z + .10) / .05) * (1 - smoothstep((z + .005) / .05))
    ear *= smoothstep((x - .04) / .035)
    width = x * .85
    fitted[:, 0] = np.sign(fitted[:, 0]) * (width + (.101 + (x - .10) * .3 - width) * ear)
    x = np.abs(fitted[:, 0])
    cheek = (1 - smoothstep((y - 1.725) / .035)) * smoothstep((z + .035) / .02)
    front = .064 + .014 * (1 - smoothstep((y - 1.665) / .055))
    angle = (z + .035) / (front + .035) * 1.62
    wrapped_x = x * np.cos(angle)
    wrapped_z = -.035 + (.173 + x - .095) * np.sin(angle)
    fitted[:, 0] += np.sign(fitted[:, 0]) * (wrapped_x - x) * cheek
    fitted[:, 1] += .025 * smoothstep((1.69 - y) / .065) * smoothstep((z + .015) / .075) * cheek
    fitted[:, 2] += (wrapped_z - z) * cheek
    throat = smoothstep((1.70 - fitted[:, 1]) / .05) * smoothstep((fitted[:, 2] + .045) / .06)
    radial = fitted[:, [0, 2]] / [.10, .17] + [0, .035 / .17]
    radius = np.linalg.norm(radial, axis=1, keepdims=True)
    neck = radial / np.maximum(radius, 1e-8) * (.75 + .25 * radius) * [.070, .078] + [0, -.042]
    fitted[:, [0, 2]] += (neck - fitted[:, [0, 2]]) * throat[:, None]
    fitted[:, 1] += (1.592 + .2 * (fitted[:, 1] - 1.65) - fitted[:, 1]) * throat
    return fitted


def rigid_weights(count, bone):
    joints = np.zeros((count, 4), dtype='<u2')
    joints[:, 0] = [BASE['nodes'][i]['name'] for i in BASE['skins'][0]['joints']].index(bone)
    weights = np.zeros((count, 4), dtype='<f4')
    weights[:, 0] = 1
    return joints, weights


def glove_warp():
    source = [[-.20, -.48], [-.18, -.90], [-.22, -.10]]
    wrist = BONES['LeftHand']
    direction = unit(BONES['LeftHandMiddle1'] - wrist)
    target = [wrist, wrist - direction * .064, wrist + direction * .052]
    fingers = {
        'Pinky': [(-.49, .18), (-.57, .40), (-.62, .56), (-.655, .705)],
        'Ring': [(-.29, .23), (-.33, .49), (-.36, .70), (-.375, .875)],
        'Middle': [(-.06, .25), (-.08, .52), (-.095, .72), (-.115, .917)],
        'Index': [(.12, .20), (.18, .43), (.225, .62), (.245, .800)],
        'Thumb': [(.16, -.29), (.32, -.12), (.49, .045), (.63, .17)],
    }
    for finger, coordinates in fingers.items():
        for i, point in enumerate(coordinates, 1):
            source.append(point)
            target.append(BONES[f'LeftHand{finger}{i}'])
    source, target = np.array(source), np.array(target)
    n = len(source)
    kernel = np.linalg.norm(source[:, None] - source[None], axis=2) ** 3
    poly = np.column_stack([np.ones(n), source])
    system = np.block([[kernel + np.eye(n) * 1e-6, poly], [poly.T, np.zeros((3, 3))]])
    coefficients = np.linalg.solve(system, np.vstack([target, np.zeros((3, 3))]))
    normal = np.array([.885, .465, 0])
    def fit(p):
        uv = p[:, [0, 2]]
        kernel = np.linalg.norm(uv[:, None] - source[None], axis=2) ** 3
        result = np.column_stack([kernel, np.ones(len(p)), uv]) @ coefficients
        center_y = np.interp(p[:, 2], [-.95, -.6, -.3, 0, .3, .6, .95], [.08, .15, .11, .015, -.05, -.12, -.245])
        return result + (p[:, 1] - center_y)[:, None] * normal * .145
    return fit


def clip_glove_cuff(positions, uv, indices, cutoff=-.5):
    _, welded = np.unique(np.round(positions, 6), axis=0, return_inverse=True)
    while True:
        edges = [tuple(sorted((a, b))) for face in welded[indices]
                 for a, b in zip(face, np.roll(face, -1))]
        counts = Counter(edges)
        incidence = np.array([counts[edge] for edge in edges]).reshape(-1, 3)
        flaps = (incidence == 1).any(axis=1) & (incidence > 2).any(axis=1)
        if not flaps.any():
            break
        indices = indices[~flaps]
    points, texcoords = list(positions), list(uv)
    intersections, triangles = {}, []
    for triangle in indices:
        polygon = []
        for a, b in zip(triangle, np.roll(triangle, -1)):
            inside_a, inside_b = positions[a, 2] >= cutoff, positions[b, 2] >= cutoff
            if inside_a:
                polygon.append(a)
            if inside_a != inside_b:
                edge = tuple(sorted((a, b)))
                if edge not in intersections:
                    t = (cutoff - positions[a, 2]) / (positions[b, 2] - positions[a, 2])
                    intersections[edge] = len(points)
                    points.append(positions[a] + t * (positions[b] - positions[a]))
                    texcoords.append(uv[a] + t * (uv[b] - uv[a]))
                polygon.append(intersections[edge])
        for i in range(1, len(polygon) - 1):
            triangles.append([polygon[0], polygon[i], polygon[i + 1]])
    used, inverse = np.unique(triangles, return_inverse=True)
    points, texcoords = np.array(points)[used], np.array(texcoords)[used]
    triangles = inverse.reshape(-1, 3)
    _, unique, welded = np.unique(np.round(points, 6), axis=0, return_index=True, return_inverse=True)
    neighbors = {}
    for triangle in triangles:
        edge = [i for i in triangle if abs(points[i, 2] - cutoff) < 1e-6]
        if len(edge) == 2:
            a, b = welded[edge]
            neighbors.setdefault(a, set()).add(b)
            neighbors.setdefault(b, set()).add(a)
    if any(len(adjacent) != 2 for adjacent in neighbors.values()):
        raise ValueError('Glove cuff cut must form a closed loop')
    loop = [min(neighbors)]
    previous = None
    while True:
        following = min(neighbors[loop[-1]] - {previous})
        if following == loop[0]:
            break
        previous = loop[-1]
        loop.append(following)
    if len(loop) != len(neighbors):
        raise ValueError('Glove cuff cut must have only one boundary')
    return points, texcoords, triangles, points[unique[loop]]


def glove_cuff(boundary):
    wrist = BONES['LeftHand']
    axis = unit(wrist - BONES['LeftForeArm'])
    normal = unit(np.array([-axis[1], axis[0], 0]))
    width = np.cross(axis, normal)
    offset = boundary - wrist
    x, y = offset @ normal, offset @ width
    if np.sum(x * np.roll(y, -1) - y * np.roll(x, -1)) < 0:
        boundary = boundary[::-1]
    offset = boundary - wrist
    polar = np.arctan2(offset @ width, offset @ normal)
    lengths = np.linalg.norm(np.roll(boundary, -1, axis=0) - boundary, axis=1)
    angle = np.r_[0, np.cumsum(lengths[:-1])] / lengths.sum() * 2 * np.pi
    angle += np.angle(np.mean(np.exp(1j * (polar - angle))))
    rings = [boundary]
    for distance, depth, breadth in [(-.028, .030, .036), (-.048, .035, .041),
                                    (-.051, .034, .040), (-.050, .031, .037)]:
        rings.append(wrist + distance * axis + np.cos(angle)[:, None] * normal * depth
                     + np.sin(angle)[:, None] * width * breadth)
    count = len(boundary)
    faces = []
    for ring in range(len(rings) - 1):
        for i in range(count):
            a, b = ring * count + i, ring * count + (i + 1) % count
            faces.extend([[a, a + count, b], [b, a + count, b + count]])
    lining = np.vstack([rings[-1], wrist - .042 * axis])
    cap = np.array([[i, count, (i + 1) % count] for i in range(count)])
    return (np.vstack(rings), np.array(faces)), (lining, cap)


def align_wrist_weights(positions, joints, weights):
    names = [BASE['nodes'][i]['name'] for i in BASE['skins'][0]['joints']]
    dense = np.zeros((len(positions), len(names)))
    np.add.at(dense, (np.arange(len(positions))[:, None], joints), weights)
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        wrist = BONES[side + 'Hand']
        axis = unit(wrist - BONES[side + 'ForeArm'])
        distance = (positions - wrist) @ axis
        radial = np.linalg.norm(positions - wrist - distance[:, None] * axis, axis=1)
        blend = smoothstep((distance + .12) / .035) * (1 - smoothstep((distance - .015) / .035))
        blend *= (positions[:, 0] * sign > 0) & (radial < .085)
        hand = smoothstep((distance + .025) / .065)
        dense *= 1 - blend[:, None]
        dense[:, names.index(side + 'ForeArm')] += blend * (1 - hand)
        dense[:, names.index(side + 'Hand')] += blend * hand
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(axis=1, keepdims=True)


def transformed_normals(p, normals, fit):
    step = 1e-4
    jacobian = np.stack([(fit(p + axis * step) - fit(p - axis * step)) / (2 * step) for axis in np.eye(3)], axis=2)
    return unit(np.linalg.solve(jacobian.transpose(0, 2, 1), normals[..., None])[..., 0])


def smooth_normals(positions, indices):
    unique, inverse = np.unique(np.round(positions, 6), axis=0, return_inverse=True)
    normals = np.zeros_like(unique)
    face = positions[indices]
    cross = np.cross(face[:, 1] - face[:, 0], face[:, 2] - face[:, 0])
    for corner in range(3):
        np.add.at(normals, inverse[indices[:, corner]], cross)
    lengths = np.linalg.norm(normals, axis=1, keepdims=True)
    return (normals / np.maximum(lengths, 1e-10))[inverse]


def tangents(positions, normals, uv, indices):
    p = positions[indices]
    tex = uv[indices]
    d1, d2 = p[:, 1] - p[:, 0], p[:, 2] - p[:, 0]
    t1, t2 = tex[:, 1] - tex[:, 0], tex[:, 2] - tex[:, 0]
    det = t1[:, 0] * t2[:, 1] - t1[:, 1] * t2[:, 0]
    reciprocal = np.zeros_like(det)
    np.divide(1, det, out=reciprocal, where=abs(det) > 1e-10)
    face_u = (d1 * t2[:, 1, None] - d2 * t1[:, 1, None]) * reciprocal[:, None]
    face_v = (d2 * t1[:, 0, None] - d1 * t2[:, 0, None]) * reciprocal[:, None]
    u, v = np.zeros_like(positions), np.zeros_like(positions)
    for corner in range(3):
        np.add.at(u, indices[:, corner], face_u)
        np.add.at(v, indices[:, corner], face_v)
    u -= normals * np.sum(normals * u, axis=1, keepdims=True)
    missing = np.linalg.norm(u, axis=1) < 1e-8
    u[missing] = np.cross(normals[missing], np.eye(3)[np.argmin(abs(normals[missing]), axis=1)])
    u = unit(u)
    handedness = np.where(np.sum(np.cross(normals, u) * v, axis=1) < 0, -1, 1)
    return np.column_stack([u, handedness])


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


def validate_closed_gloves(doc, binary):
    positions, triangles = [], []
    for primitive in doc['meshes'][0]['primitives']:
        points = accessor(doc, binary, primitive['attributes']['POSITION'])
        faces = accessor(doc, binary, primitive['indices']).reshape(-1, 3)
        triangles.extend(faces + len(positions))
        positions.extend(points)
    positions, triangles = np.array(positions), np.array(triangles)
    _, welded = np.unique(np.round(positions, 6), axis=0, return_inverse=True)
    directed = Counter((a, b) for face in welded[triangles] for a, b in zip(face, np.roll(face, -1)))
    if any(count != 1 or directed[b, a] != 1 for (a, b), count in directed.items()):
        raise ValueError('Rebuilt gloves must be closed with consistent face winding')
    face = positions[triangles]
    if np.any(np.linalg.norm(np.cross(face[:, 1] - face[:, 0], face[:, 2] - face[:, 0]), axis=1) < 1e-12):
        raise ValueError('Rebuilt gloves contain degenerate triangles')


def build(name, fit):
    doc, raw = read_glb(PARTS / 'knight/sources' / f'{name}.glb')
    for material in doc['materials']:
        material['normalTexture']['scale'] = .3
    binary = bytearray(raw)
    for primitive in doc['meshes'][0]['primitives']:
        attributes = primitive['attributes']
        source = accessor(doc, raw, attributes['POSITION'])
        indices = accessor(doc, raw, primitive['indices']).reshape(-1, 3)
        uv = accessor(doc, raw, attributes['TEXCOORD_0'])
        if name == 'pants_plate':
            source, uv, indices = subdivide_greave_hems(source, uv, indices)
            attributes['TEXCOORD_0'] = add_accessor(doc, binary, uv, 'VEC2')
        if name == 'helmet_plate':
            source, uv, indices = subdivide_helmet_cheeks(source, uv, indices)
            attributes['TEXCOORD_0'] = add_accessor(doc, binary, uv, 'VEC2')
        if name == 'gloves_plate':
            source, uv, indices, boundary = clip_glove_cuff(source, uv, indices)
        positions = fit(source)
        normals = transformed_normals(source, smooth_normals(source, indices), fit)
        if name == 'gloves_plate':
            indices = indices[:, ::-1]
            positions = np.vstack([positions, positions * [-1, 1, 1]])
            normals = np.vstack([normals, normals * [-1, 1, 1]])
            indices = np.vstack([indices, indices[:, ::-1] + len(source)])
            uv = np.vstack([uv, uv])
            attributes['TEXCOORD_0'] = add_accessor(doc, binary, uv, 'VEC2')
        if name == 'helmet_plate':
            joints, weights = rigid_weights(len(positions), 'Head')
        else:
            joints, weights = transfer_weights(positions)
        if name == 'pants_plate':
            joints, weights = align_crotch_weights(positions, joints, weights)
        if name in ('pants_plate', 'boots_plate'):
            joints, weights = align_ankle_weights(positions, joints, weights)
        if name in ('top_plate', 'gloves_plate'):
            joints, weights = align_wrist_weights(positions, joints, weights)
        attributes.update(add_skin_attributes(doc, binary, positions, normals, joints, weights))
        attributes['TANGENT'] = add_accessor(doc, binary, tangents(positions, normals, uv, indices), 'VEC4')
        primitive['indices'] = add_accessor(doc, binary, indices.reshape(-1, 1), 'SCALAR', 5125)
    if name == 'gloves_plate':
        materials = [
            {'name': 'Steel cuff', 'pbrMetallicRoughness': {'baseColorFactor': [.42, .44, .45, 1],
             'metallicFactor': .95, 'roughnessFactor': .27}},
            {'name': 'Padded cuff lining', 'pbrMetallicRoughness': {'baseColorFactor': [.035, .040, .045, 1],
             'metallicFactor': 0, 'roughnessFactor': .9}},
        ]
        for (left, faces), material in zip(glove_cuff(fit(boundary)), materials):
            positions = np.vstack([left, left * [-1, 1, 1]])
            indices = np.vstack([faces, faces[:, ::-1] + len(left)])
            normals = smooth_normals(positions, indices)
            joints, weights = transfer_weights(positions)
            joints, weights = align_wrist_weights(positions, joints, weights)
            attributes = add_skin_attributes(doc, binary, positions, normals, joints, weights)
            doc['meshes'][0]['primitives'].append({'attributes': attributes,
                'indices': add_accessor(doc, binary, indices.reshape(-1, 1), 'SCALAR', 5125),
                'material': len(doc['materials'])})
            doc['materials'].append(material)
        validate_closed_gloves(doc, binary)
    original = [i for i, node in enumerate(BASE['nodes']) if 'mesh' not in node]
    remap = {old: new for new, old in enumerate(original)}
    doc['nodes'] = [copy.deepcopy(BASE['nodes'][i]) for i in original]
    for node in doc['nodes']:
        if 'children' in node:
            node['children'] = [remap[i] for i in node['children'] if i in remap]
    skin = copy.deepcopy(BASE['skins'][0])
    skin['joints'] = [remap[i] for i in skin['joints']]
    skin['inverseBindMatrices'] = add_accessor(doc, binary, accessor(BASE, BASE_BIN, BASE['skins'][0]['inverseBindMatrices']), 'MAT4')
    doc['skins'] = [skin]
    doc['scenes'] = copy.deepcopy(BASE['scenes'])
    for scene in doc['scenes']:
        scene['nodes'] = [remap[i] for i in scene['nodes'] if i in remap]
    doc['scenes'][0]['nodes'].append(len(doc['nodes']))
    extras = {'rig_id': 'human_male_01_mixamo_candidate_v2', 'part_id': name, 'region': name}
    doc['nodes'].append({'name': name, 'mesh': 0, 'skin': 0, 'extras': extras})
    if name in ('gloves_plate', 'helmet_plate'):
        doc['meshes'][0]['extras'] = extras
    path = PARTS / 'fitted' / f'{name}.glb'
    write_glb(path, doc, compact(doc, binary))
    triangles = sum(doc['accessors'][p['indices']]['count'] // 3 for p in doc['meshes'][0]['primitives'])
    print(name, 'triangles', triangles)


if __name__ == '__main__':
    for name, fit in [('top_plate', top_fit), ('pants_plate', pants_fit), ('boots_plate', boots_fit), ('helmet_plate', helmet_fit), ('gloves_plate', glove_warp())]:
        build(name, fit)
