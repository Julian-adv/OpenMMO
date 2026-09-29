import copy
import importlib.util
import subprocess
from functools import cache
from pathlib import Path

import numpy as np

from lib.glb import read_glb, write_glb

ROOT = Path(__file__).resolve().parents[1]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
spec = importlib.util.spec_from_file_location('plate', ROOT / 'tools/fit-modular-plate.py')
plate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plate)


def with_rig(doc, binary, name):
    base = plate.BASE
    original = [i for i, node in enumerate(base['nodes']) if 'mesh' not in node]
    remap = {old: new for new, old in enumerate(original)}
    doc['nodes'] = [copy.deepcopy(base['nodes'][i]) for i in original]
    for node in doc['nodes']:
        if 'children' in node:
            node['children'] = [remap[i] for i in node['children'] if i in remap]
    skin = copy.deepcopy(base['skins'][0])
    skin['joints'] = [remap[i] for i in skin['joints']]
    if 'skeleton' in skin:
        skin['skeleton'] = remap[skin['skeleton']]
    skin['inverseBindMatrices'] = plate.add_accessor(doc, binary,
        plate.accessor(base, plate.BASE_BIN, base['skins'][0]['inverseBindMatrices']), 'MAT4')
    doc['skins'] = [skin]
    doc['scenes'] = copy.deepcopy(base['scenes'])
    for scene in doc['scenes']:
        scene['nodes'] = [remap[i] for i in scene['nodes'] if i in remap]
    extras = {'rig_id': 'human_male_01_mixamo_candidate_v2', 'part_id': name, 'region': name}
    doc['scenes'][0]['nodes'].append(len(doc['nodes']))
    doc['nodes'].append({'name': name, 'mesh': 0, 'skin': 0, 'extras': extras})
    doc['meshes'][0]['name'] = name
    doc['meshes'][0]['extras'] = extras
    doc.pop('animations', None)


def build(name, fit, skinning, mirror=False):
    doc, raw = read_glb(PARTS / 'barbarian_sources' / f'{name}.glb')
    if len(doc['meshes']) != 1:
        raise ValueError(f'{name}: expected a single source mesh')
    binary = bytearray(raw)
    for material in doc['materials']:
        if 'normalTexture' in material:
            material['normalTexture']['scale'] = .5
        material.pop('emissiveTexture', None)
        material.pop('emissiveFactor', None)
    for primitive in doc['meshes'][0]['primitives']:
        attrs = primitive['attributes']
        source = plate.accessor(doc, raw, attrs['POSITION'])
        uv = plate.accessor(doc, raw, attrs['TEXCOORD_0'])
        indices = plate.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
        positions = fit(source)
        normals = plate.smooth_normals(positions, indices)
        if name == 'pants_barbarian':
            centers = source[indices].mean(axis=1)
            faces = indices[(centers[:, 1] < .55) & (abs(centers[:, 0]) < .40) &
                            (centers[:, 2] > .12)]
            used, mapped = np.unique(faces, return_inverse=True)
            back = positions[used].copy()
            back[:, 2] = -back[:, 2] - .052
            back[:, 0] *= 1.12
            indices = np.vstack([indices, mapped.reshape(-1, 3)[:, ::-1] + len(positions)])
            positions = np.vstack([positions, back])
            normals = np.vstack([normals, normals[used] * [1, 1, -1]])
            uv = np.vstack([uv, uv[used]])
        if mirror:
            positions = np.vstack([positions, positions * [-1, 1, 1]])
            normals = np.vstack([normals, normals * [-1, 1, 1]])
            uv = np.vstack([uv, uv])
            indices = np.vstack([indices, indices[:, ::-1] + len(source)])
        joints, weights = skinning(positions)
        attrs.clear()
        attrs.update(plate.add_skin_attributes(doc, binary, positions, normals, joints, weights))
        attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
        attrs['TANGENT'] = plate.add_accessor(doc, binary,
            plate.tangents(positions, normals, uv, indices), 'VEC4')
        primitive['indices'] = plate.add_accessor(doc, binary, indices.reshape(-1, 1), 'SCALAR', 5125)
    if name == 'pants_barbarian':
        add_lining(doc, binary)
        add_back_belt(doc, binary)
    if name in ('boots_barbarian', 'gloves_barbarian'):
        add_cuff(doc, binary, name)
    if name == 'boots_barbarian':
        add_cuff(doc, binary, name, shaft=True)
    with_rig(doc, binary, name)
    path = PARTS / 'fitted' / f'{name}.glb'
    write_glb(path, doc, plate.compact(doc, binary))
    triangles = sum(doc['accessors'][p['indices']]['count'] // 3 for p in doc['meshes'][0]['primitives'])
    print(name, 'triangles', triangles)


def top_fit(points):
    result = points * [.36, .60, .29] + [0, 1.51, -.035]
    back = plate.smoothstep((-points[:, 2] - .02) / .35)
    result[:, 1] -= .030 * back
    result[:, 2] -= .005 * back
    return result


def pants_fit(points):
    return points * [.37, .25, .44] + [0, .927, -.025]


def helmet_fit(points):
    return points * [.23, .18, .21] + [0, 1.843, -.006]


def boots_fit(points):
    y = points[:, 1] * .19 + .295
    source = source_points('boots_barbarian')
    source_center, source_radius = cross_sections(source[:, 1], source[:, [0, 2]], points[:, 1], .10)
    center, radius = leg_sections(y)
    local = (points[:, [0, 2]] - source_center) / source_radius
    length = np.linalg.norm(local, axis=1, keepdims=True)
    local *= np.maximum(length, 1.1) / np.maximum(length, 1e-5)
    cross = center + local * (radius + .012)
    return np.column_stack([cross[:, 0], y, cross[:, 1]])


@cache
def source_points(name):
    doc, raw = read_glb(PARTS / 'barbarian_sources' / f'{name}.glb')
    return np.concatenate([plate.accessor(doc, raw, p['attributes']['POSITION'])
                           for p in doc['meshes'][0]['primitives']])


def cross_sections(axis, cross, values, band):
    samples = np.linspace(axis.min(), axis.max(), 30)
    low, high = [], []
    for sample in samples:
        points = cross[abs(axis - sample) < band]
        if not len(points):
            points = cross[np.argsort(abs(axis - sample))[:8]]
        low.append(points.min(0))
        high.append(points.max(0))
    center = (np.array(low) + high) / 2
    radius = np.maximum((np.array(high) - low) / 2, .005)
    return tuple(np.column_stack([np.interp(values, samples, data[:, i]) for i in range(2)])
                 for data in (center, radius))


def leg_sections(y):
    body = plate.BODY_POS[(plate.BODY_POS[:, 0] > .06) &
                          (plate.BODY_POS[:, 1] < .56)]
    return cross_sections(body[:, 1], body[:, [0, 2]], y, .025)


def arm_sections(t):
    elbow, wrist = plate.BONES['LeftForeArm'], plate.BONES['LeftHand']
    axis = plate.unit(wrist - elbow)
    width = plate.unit(np.cross(np.array([0, 0, 1.]), axis))
    body = plate.BODY_POS[plate.BODY_POS[:, 0] > .25] - elbow
    along = body @ axis / np.linalg.norm(wrist - elbow)
    return cross_sections(along, np.column_stack([body @ width, body[:, 2]]), t, .07)


def gloves_fit(points):
    elbow, wrist = plate.BONES['LeftForeArm'], plate.BONES['LeftHand']
    axis = plate.unit(wrist - elbow)
    front = np.array([0, 0, 1.])
    width = plate.unit(np.cross(front, axis))
    t = (.95 - points[:, 1]) / 1.90 * .71 + .22
    center, radius = arm_sections(t)
    depth = center[:, 1] + radius[:, 1] + .008
    return (elbow + (wrist - elbow) * t[:, None] +
            (depth + points[:, 2] * .065)[:, None] * front +
            (center[:, 0] + points[:, 0] / .465 * (radius[:, 0] + .01))[:, None] * width)


def paired_rigid_weights(points, bone):
    joints, weights = plate.rigid_weights(len(points), 'Left' + bone)
    right, _ = plate.rigid_weights(len(points), 'Right' + bone)
    joints[points[:, 0] < 0] = right[points[:, 0] < 0]
    return joints, weights


def pants_weights(points):
    joints, weights = plate.rigid_weights(len(points), 'Hips')
    left, _ = plate.rigid_weights(len(points), 'LeftUpLeg')
    right, _ = plate.rigid_weights(len(points), 'RightUpLeg')
    lower = plate.smoothstep((1.06 - points[:, 1]) / .29) * .45
    side = plate.smoothstep((points[:, 0] + .09) / .18)
    joints[:, 1], joints[:, 2] = left[:, 0], right[:, 0]
    weights[:, 0] = 1 - lower
    weights[:, 1], weights[:, 2] = lower * side, lower * (1 - side)
    return joints, weights


def add_lining(doc, binary):
    material = len(doc['materials'])
    texture = reference_texture(doc, binary, 'barbarian_leather')
    doc['materials'].append({'name': 'Brown leather loincloth lining',
        'pbrMetallicRoughness': {'baseColorTexture': {'index': texture},
                                'metallicFactor': 0, 'roughnessFactor': .92}})
    for node in plate.BASE['nodes']:
        if node.get('extras', {}).get('region') != 'legs' or 'mesh' not in node:
            continue
        for primitive in plate.BASE['meshes'][node['mesh']]['primitives']:
            attrs = primitive['attributes']
            positions = plate.accessor(plate.BASE, plate.BASE_BIN, attrs['POSITION'])
            faces = plate.accessor(plate.BASE, plate.BASE_BIN, primitive['indices']).reshape(-1, 3)
            faces = faces[positions[faces, 1].max(axis=1) > .795]
            used, mapped = np.unique(faces, return_inverse=True)
            normals = plate.accessor(plate.BASE, plate.BASE_BIN, attrs['NORMAL'])[used]
            points = positions[used] + normals * .004
            points[:, 1] = np.maximum(points[:, 1], .795)
            joints = plate.accessor(plate.BASE, plate.BASE_BIN, attrs['JOINTS_0'])[used]
            weights = plate.accessor(plate.BASE, plate.BASE_BIN, attrs['WEIGHTS_0'])[used]
            attrs = plate.add_skin_attributes(doc, binary, points, normals, joints, weights)
            uv = np.column_stack([np.arctan2(points[:, 2], points[:, 0]) / np.pi * 2,
                                  (1.1 - points[:, 1]) * 5])
            attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
            doc['meshes'][0]['primitives'].append({
                'attributes': attrs,
                'indices': plate.add_accessor(doc, binary, mapped.reshape(-1, 1), 'SCALAR', 5125),
                'material': material,
            })


def reference_texture(doc, binary, name):
    for i, image in enumerate(doc['images']):
        if image.get('name') == name:
            return next(j for j, texture in enumerate(doc['textures']) if texture.get('source') == i)
    image_path = ROOT / f'doc/images/characters/modular_human_male_01/parts/{name}.png'
    data = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(image_path),
        '-vf', 'scale=1024:1024:flags=lanczos', '-f', 'image2pipe', '-vcodec', 'png', '-'])
    binary.extend(b'\0' * (-len(binary) % 4))
    view = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(data)})
    binary.extend(data)
    image = len(doc['images'])
    doc['images'].append({'bufferView': view, 'mimeType': 'image/png', 'name': name})
    texture = len(doc['textures'])
    doc['textures'].append({'source': image})
    return texture


def add_cuff(doc, binary, name, shaft=False):
    fur = name == 'boots_barbarian' and not shaft
    texture = reference_texture(doc, binary, 'barbarian_fur' if fur else 'barbarian_leather')
    material = len(doc['materials'])
    pbr = {'metallicFactor': 0, 'roughnessFactor': .95, 'baseColorTexture': {'index': texture}}
    doc['materials'].append({'name': 'Tawny fur cuff' if fur else 'Leather bracer sleeve',
                              'pbrMetallicRoughness': pbr, 'doubleSided': True})
    points, uv = [], []
    count = 32 if fur else 20
    rows = 5 if fur or shaft else 9
    for row in range(rows):
        for i in range(count + 1):
            angle = i / count * 2 * np.pi
            if fur or shaft:
                y = ([.398, .43, .478, .482, .405] if fur else [.13, .20, .29, .38, .44])[row]
                padding = [.020, .033, .026, .010, .010][row] if fur else .009
                if fur and row == 0:
                    y += .008 * np.cos(angle * 11)
                    padding += .005 * np.cos(angle * 11)
                center, radius = leg_sections(np.array([y]))
                cross = center[0] + [np.cos(angle), np.sin(angle)] * (radius[0] + padding)
                point = np.array([cross[0], y, cross[1]])
                tex = [i / count * 3, row / 4 * .7 if fur else (.44 - y) * 5]
            else:
                elbow, wrist = plate.BONES['LeftForeArm'], plate.BONES['LeftHand']
                axis = plate.unit(wrist - elbow)
                width = plate.unit(np.cross(np.array([0, 0, 1.]), axis))
                t = [.20, .22, .35, .50, .65, .80, .92, .94, .94][row]
                center, radius = arm_sections(np.array([t]))
                padding = [.012, .011, .011, .011, .011, .013, .015, .016, .008][row]
                cross = center[0] + [np.cos(angle), np.sin(angle)] * (radius[0] + padding)
                point = elbow + (wrist - elbow) * t + width * cross[0]
                point += np.array([0, 0, 1.]) * cross[1]
                tex = [i / count * 2, t * 1.5]
            points.append(point)
            uv.append(tex)
    faces = []
    for row in range(rows - 1):
        for i in range(count):
            a, b = row * (count + 1) + i, (row + 1) * (count + 1) + i
            faces.extend([[a, b, a + 1], [a + 1, b, b + 1]])
    points, faces, uv = np.array(points), np.array(faces), np.array(uv)
    if not fur and not shaft:
        faces = faces[:, ::-1]
    normals = plate.smooth_normals(points, faces)
    faces = np.vstack([faces, faces[:, ::-1] + len(points)])
    points = np.vstack([points, points * [-1, 1, 1]])
    normals = np.vstack([normals, normals * [-1, 1, 1]])
    uv = np.vstack([uv, uv])
    joints, weights = paired_rigid_weights(points, 'Leg' if fur or shaft else 'ForeArm')
    attrs = plate.add_skin_attributes(doc, binary, points, normals, joints, weights)
    attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
    doc['meshes'][0]['primitives'].append({'attributes': attrs,
        'indices': plate.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125),
        'material': material})


def add_back_belt(doc, binary):
    points, faces, uv = [], [], []
    count = 24
    for row in range(4):
        y = [1.065, 1.14, 1.14, 1.065][row]
        inset = .004 if row > 1 else 0
        for i in range(count + 1):
            angle = np.pi + i / count * np.pi
            points.append([np.cos(angle) * (.193 - inset), y,
                           -.012 + np.sin(angle) * (.173 - inset)])
            uv.append([i / count * 3, (1.14 - y) * 5])
    for row in range(3):
        for i in range(count):
            a, b = row * (count + 1) + i, (row + 1) * (count + 1) + i
            faces.extend([[a, b, a + 1], [a + 1, b, b + 1]])
    points, faces = np.array(points), np.array(faces)
    normals = plate.smooth_normals(points, faces)
    joints, weights = plate.rigid_weights(len(points), 'Hips')
    attrs = plate.add_skin_attributes(doc, binary, points, normals, joints, weights)
    attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
    doc['meshes'][0]['primitives'].append({
        'attributes': attrs,
        'indices': plate.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125),
        'material': len(doc['materials']) - 1,
    })


if __name__ == '__main__':
    build('helmet_barbarian', helmet_fit, lambda p: plate.rigid_weights(len(p), 'Head'))
    build('top_barbarian', top_fit, plate.transfer_weights)
    build('pants_barbarian', pants_fit, pants_weights)
    build('boots_barbarian', boots_fit, lambda p: paired_rigid_weights(p, 'Leg'), mirror=True)
    build('gloves_barbarian', gloves_fit, lambda p: paired_rigid_weights(p, 'ForeArm'), mirror=True)
