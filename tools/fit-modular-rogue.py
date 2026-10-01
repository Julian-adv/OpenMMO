"""Build the first rogue fitting candidate against the unchanged modular male rig."""
import argparse
import copy
import hashlib
import importlib.util
import json
import sys
from pathlib import Path
from functools import cache

import numpy as np
from scipy.interpolate import RBFInterpolator
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb
from outfits.body_shape import slim_calves
from outfits.rogue_layers import build_top, build_wrap

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('plate_io', ROOT / 'tools/fit-modular-plate.py')
io = importlib.util.module_from_spec(spec)
spec.loader.exec_module(io)
NAMES = [io.BASE['nodes'][i]['name'] for i in io.BASE['skins'][0]['joints']]
BONES = io.BONES
SELECTION = json.loads((ROOT / 'doc/assets/modular-rogue-source-selection.json').read_text())
OUTPUT = ROOT / SELECTION['fitting_candidate']['directory']
REPORT = ROOT / SELECTION['fitting_candidate']['report']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


@cache
def body_surface(regions, side=0):
    points, faces, weights = [], [], []
    for node in io.BASE['nodes']:
        if node.get('extras', {}).get('region') not in regions:
            continue
        for primitive in io.BASE['meshes'][node['mesh']]['primitives']:
            attrs = primitive['attributes']
            p = io.accessor(io.BASE, io.BASE_BIN, attrs['POSITION'])
            f = io.accessor(io.BASE, io.BASE_BIN, primitive['indices']).reshape(-1, 3)
            if side:
                f = f[p[f].mean(1)[:, 0] * side > 0]
            faces.extend(f + len(points))
            points.extend(p)
            joints = io.accessor(io.BASE, io.BASE_BIN, attrs['JOINTS_0'])
            w = io.accessor(io.BASE, io.BASE_BIN, attrs['WEIGHTS_0'])
            dense = np.zeros((len(p), len(NAMES)))
            np.add.at(dense, (np.arange(len(p))[:, None], joints), w)
            weights.extend(dense)
    return np.asarray(points), np.asarray(faces), np.asarray(weights)


def nearest_surface(points, surface, candidates=32):
    vertices, faces, weights = surface
    triangles = vertices[faces]
    near = cKDTree(triangles.mean(1)).query(points, k=min(candidates, len(faces)))[1]
    a, b, c = np.moveaxis(triangles[near], 2, 0)
    ab, ac, ap = b - a, c - a, points[:, None] - a
    dot = lambda x, y: np.sum(x * y, axis=-1)
    d00, d01, d11 = dot(ab, ab), dot(ab, ac), dot(ac, ac)
    d20, d21 = dot(ap, ab), dot(ap, ac)
    denom = np.maximum(d00 * d11 - d01 * d01, 1e-20)
    v, w = (d11 * d20 - d01 * d21) / denom, (d00 * d21 - d01 * d20) / denom
    bary = np.stack([1 - v - w, v, w], axis=-1)
    projected = a + v[..., None] * ab + w[..., None] * ac
    distances = np.where(np.min(bary, axis=-1) >= 0, dot(projected - points[:, None], projected - points[:, None]), np.inf)
    candidates, barycentrics, ds = [projected], [bary], [distances]
    for i, j, start, end in [(0, 1, a, b), (1, 2, b, c), (2, 0, c, a)]:
        edge = end - start
        t = np.clip(dot(points[:, None] - start, edge) / np.maximum(dot(edge, edge), 1e-20), 0, 1)
        q = start + t[..., None] * edge
        bc = np.zeros_like(bary)
        bc[..., i], bc[..., j] = 1 - t, t
        candidates.append(q)
        barycentrics.append(bc)
        ds.append(dot(q - points[:, None], q - points[:, None]))
    choices = np.concatenate(ds, axis=1).argmin(1)
    rows = np.arange(len(points))
    q = np.concatenate(candidates, axis=1)[rows, choices]
    bc = np.concatenate(barycentrics, axis=1)[rows, choices]
    f = near[rows, choices % near.shape[1]]
    normal = io.unit(np.cross(triangles[f, 1] - triangles[f, 0], triangles[f, 2] - triangles[f, 0]))
    dense = (weights[faces[f]] * bc[..., None]).sum(1)
    return q, normal, dense


def transfer(points, surface, allowed):
    _, _, dense = nearest_surface(points, surface)
    dense[:, [i for i, name in enumerate(NAMES) if name not in allowed]] = 0
    empty = dense.sum(1) < 1e-8
    if empty.any():
        raise ValueError(f'{empty.sum()} vertices have no permitted skin influence')
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    weights /= weights.sum(1, keepdims=True)
    return joints, weights


def repair_uv(points, uv, faces):
    t = uv[faces]
    ab, ac = t[:, 1] - t[:, 0], t[:, 2] - t[:, 0]
    good = abs(ab[:, 0] * ac[:, 1] - ab[:, 1] * ac[:, 0]) > 1e-10
    if good.all():
        return points, uv, faces, [], np.arange(len(points))
    centers = points[faces].mean(1)
    normals = io.unit(np.cross(points[faces[:, 1]] - points[faces[:, 0]], points[faces[:, 2]] - points[faces[:, 0]]))
    new_points, new_uv = list(points), list(uv)
    faces = faces.copy()
    repaired = []
    vertex_map = list(range(len(points)))
    for index in np.where(~good)[0]:
        distances = np.linalg.norm(centers - centers[index], axis=1)
        distances[~good | (normals @ normals[index] < .4)] = np.inf
        neighbor = distances.argmin()
        if not good[neighbor] or not np.isfinite(distances[neighbor]):
            raise ValueError('No adjacent UV patch for repair')
        a, b, c = points[faces[neighbor]]
        local = np.linalg.lstsq(np.column_stack([b - a, c - a]), (points[faces[index]] - a).T, rcond=None)[0].T
        ta, tb, tc = uv[faces[neighbor]]
        bary = np.column_stack([1 - local.sum(1), local])
        factor = min(1., .31 / max(1 / 3 - bary.min(), 1e-6))
        bary = 1 / 3 + (bary - 1 / 3) * factor
        corrected = bary @ np.array([ta, tb, tc])
        start = len(new_points)
        vertex_map.extend(faces[index])
        new_points.extend(points[faces[index]])
        new_uv.extend(corrected)
        faces[index] = np.arange(start, start + 3)
        repaired.append(dict(face=int(index), reference_face=int(neighbor)))
    return np.array(new_points), np.array(new_uv), faces, repaired, np.array(vertex_map)


def pants_initial(p):
    out = p * [.43, .501, .48] + [0, .67, -.025]
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        mask = p[:, 0] * sign > 0
        t = io.smoothstep((.94 - out[:, 1]) / .5)
        out[mask, 0] += sign * .035 * t[mask]
    waist = io.smoothstep((out[:, 1] - .98) / .13)
    out[:, 0] *= 1 + .07 * waist
    out[:, 2] = -.015 + (out[:, 2] + .015) * (1 + .14 * waist)
    return out


def boot_initial(p):
    out = np.column_stack([.1664 + p[:, 2] * .17, (p[:, 1] + .606) * .195 + .003, -p[:, 0] * .18 + .034])
    return out


@cache
def ankle_interface(side):
    selection = SELECTION
    path = ROOT / selection['reference']['interfaces']
    data = json.loads(path.read_text())
    assert data['base_sha256'] == selection['reference']['base_sha256']
    ref = next(i for i in data['interfaces'] if i['name'] == 'shoe_ankle_' + side)
    center, axis = np.array(ref['center']), io.unit(np.array(ref['normal']))
    across = io.unit(np.array([1., 0, 0]) - axis * axis[0])
    forward = np.cross(across, axis)
    basis = np.array([across, forward])
    profiles = []
    for name in ['band_minus', 'center', 'band_plus']:
        points = np.array(ref['contours'][name]['points'])
        cross = (points - center) @ basis.T
        angles = np.arctan2(cross[:, 1], cross[:, 0])
        order = np.argsort(angles)
        profiles.append((np.mean((points - center) @ axis), angles[order], np.linalg.norm(cross, axis=1)[order]))
    points = np.array(ref['contours']['center']['points'])
    _, _, weights = nearest_surface(points, body_surface(('legs', 'ankles', 'feet'), 1 if side == 'Left' else -1))
    dense = np.zeros(len(NAMES))
    for bone in ['Leg', 'Foot']:
        index = NAMES.index(side + bone)
        dense[index] = weights[:, index].mean()
    dense /= dense.sum()
    return center, axis, basis, profiles, dense


def skin_radius(side, height, angles):
    profiles = ankle_interface(side)[3]
    radii = np.stack([np.interp(angles, theta, radius, period=2 * np.pi) for _, theta, radius in profiles])
    below = np.clip((height - profiles[0][0]) / -profiles[0][0], 0, 1)
    above = np.clip(height / profiles[2][0], 0, 1)
    return np.where(height < 0, radii[0] * (1 - below) + radii[1] * below,
                    radii[1] * (1 - above) + radii[2] * above)


@cache
def ankle_source_profiles(kind, side):
    selection = SELECTION
    name = 'pants_rogue' if kind == 'pants' else 'boot_rogue_left'
    path = next(p['source_glb'] for p in selection['parts'] if p['id'] == name)
    doc, binary = read_glb(ROOT / path)
    prim = doc['meshes'][0]['primitives'][0]
    p = io.accessor(doc, binary, prim['attributes']['POSITION'])
    p = pants_initial(p) if kind == 'pants' else boot_initial(p)
    if kind == 'boot' and side == 'Right':
        p[:, 0] *= -1
    faces = io.accessor(doc, binary, prim['indices']).reshape(-1, 3)
    triangles = p[faces]
    triangles = triangles[triangles.mean(1)[:, 0] * (1 if side == 'Left' else -1) > 0]
    heights = np.array([.208, .23, .26, .30, .35, .40]) if kind == 'pants' else np.array([.13, .16, .185, .205, .215])
    profiles = []
    for y in heights:
        low, high = io.section_bounds(triangles, np.array([0., y, 0.]),
            np.array([0., 1., 0.]), np.array([[1., 0., 0.], [0., 0., 1.]]))
        center = (low + high) / 2
        profiles.append((center, (high - low) / 2))
    return heights, np.array([p[0] for p in profiles]), np.array([p[1] for p in profiles])


def fit_ankle(points, kind, side):
    center, axis, basis, _, _ = ankle_interface(side)
    heights, centers, radii = ankle_source_profiles(kind, side)
    y = points[:, 1]
    origin = np.column_stack([np.interp(y, heights, centers[:, i]) for i in range(2)])
    extent = np.column_stack([np.interp(y, heights, radii[:, i]) for i in range(2)])
    cross = (points[:, [0, 2]] - origin) / extent
    angle = np.arctan2(cross[:, 1], cross[:, 0])
    radius = np.linalg.norm(cross, axis=1)
    if kind == 'pants':
        height = y - center[1] - .004
        clearance = .004 + np.clip((radius - 1) * .012, -.001, .001)
        influence = 1 - io.smoothstep((y - .27) / .13)
    else:
        height = np.interp(y, [.12, .17, .205, .24], [-.115, -.065, -.010, .006])
        clearance = .032 + np.clip((radius - 1) * .045, -.008, .006)
        influence = io.smoothstep((y - .13) / .055)
    radial = skin_radius(side, height, angle) + clearance
    target = center + height[:, None] * axis
    target += radial[:, None] * (np.cos(angle)[:, None] * basis[0] + np.sin(angle)[:, None] * basis[1])
    return points * (1 - influence[:, None]) + target * influence[:, None]


def pants_fit(p):
    out = pants_initial(p)
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        mask = out[:, 0] * sign > 0
        out[mask] = fit_ankle(out[mask], 'pants', side)
    return slim_calves(out, BONES)


def boot_fit(p, side='Left'):
    out = boot_initial(p)
    if side == 'Right':
        out[:, 0] *= -1
    return fit_ankle(out, 'boot', side)


def ankle_weights(points, joints, weights):
    dense = np.zeros((len(points), len(NAMES)))
    np.add.at(dense, (np.arange(len(points))[:, None], joints), weights)
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        center, axis, _, _, shared = ankle_interface(side)
        h = (points - center) @ axis
        blend = io.smoothstep((h + .10) / .045) * (1 - io.smoothstep((h - .065) / .065))
        blend *= points[:, 0] * sign > 0
        dense = dense * (1 - blend[:, None]) + shared * blend[:, None]
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(1, keepdims=True)



def add_ankle_lining(doc, binary, name):
    boot = name == 'boots_rogue'
    points, faces, normals, joints, weights = [], [], [], [], []
    count = 32
    angles = np.arange(count + 1) / count * 2 * np.pi
    for side in ['Left', 'Right']:
        center, axis, basis, _, shared = ankle_interface(side)
        start = len(points)
        for height in ([-.035, .004] if boot else [-.040, -.012]):
            radius = skin_radius(side, np.full(len(angles), height), angles) + (.018 if boot else .002)
            direction = np.cos(angles)[:, None] * basis[0] + np.sin(angles)[:, None] * basis[1]
            points.extend(center + height * axis + radius[:, None] * direction)
            normals.extend(direction * (-1 if boot else 1))
            top = np.argsort(shared)[-4:][::-1]
            joints.extend([top] * len(angles))
            weights.extend([shared[top]] * len(angles))
        for i in range(count):
            a, b = start + i, start + i + count + 1
            triangles = [[a, a + 1, b], [a + 1, b + 1, b]]
            faces.extend(triangles if boot else [t[::-1] for t in triangles])
        if boot:
            inner = len(points)
            points.append(center - .035 * axis)
            normals.append(axis)
            joints.append(top)
            weights.append(shared[top])
            for i in range(count):
                a = start + i
                faces.append([a, inner, a + 1])
    material = len(doc['materials'])
    doc['materials'].append(dict(name='boot_inner_collar' if boot else 'trouser_inner_hem', doubleSided=True,
        pbrMetallicRoughness=dict(baseColorFactor=[.012, .009, .006, 1] if boot else [.012, .014, .018, 1], metallicFactor=0, roughnessFactor=.9)))
    attrs = io.add_skin_attributes(doc, binary, np.array(points), np.array(normals), np.array(joints), np.array(weights))
    doc['meshes'].append(dict(name=name + '_inner_collar', primitives=[dict(attributes=attrs, material=material,
        indices=io.add_accessor(doc, binary, np.array(faces).reshape(-1, 1), 'SCALAR', 5125))]))


def ray_bounds(centers, normal, triangles, limit):
    a, b, c = triangles[:, 0], triangles[:, 1], triangles[:, 2]
    e1, e2 = b - a, c - a
    h = np.cross(np.broadcast_to(normal, e2.shape), e2)
    det = np.sum(e1 * h, axis=1)
    inv = np.divide(1., det, out=np.zeros_like(det), where=abs(det) > 1e-10)
    lows, highs = np.full(len(centers), np.nan), np.full(len(centers), np.nan)
    for start in range(0, len(centers), 64):
        offset = centers[start:start + 64, None] - a
        u = np.sum(offset * h, axis=2) * inv
        q = np.cross(offset, e1)
        v = (q @ normal) * inv
        distance = np.sum(e2 * q, axis=2) * inv
        valid = (abs(det) > 1e-10) & (u >= -1e-5) & (v >= -1e-5) & (u + v <= 1 + 1e-5) & (abs(distance) < limit)
        lo = np.where(valid, distance, np.inf).min(1)
        hi = np.where(valid, distance, -np.inf).max(1)
        hit = np.isfinite(lo) & np.isfinite(hi) & (hi - lo > .005)
        rows = start + np.where(hit)[0]
        lows[rows], highs[rows] = lo[hit], hi[hit]
    return lows, highs


@cache
def glove_triangles():
    selection = SELECTION
    path = next(p['source_glb'] for p in selection['parts'] if p['id'] == 'glove_rogue_right')
    doc, binary = read_glb(ROOT / path)
    prim = doc['meshes'][0]['primitives'][0]
    p = io.accessor(doc, binary, prim['attributes']['POSITION'])
    f = io.accessor(doc, binary, prim['indices']).reshape(-1, 3)
    return p[f]


def glove_fit(p, volume=True):
    wrist = BONES['RightHand']
    axis = io.unit(wrist - BONES['RightForeArm'])
    source = [[0, .95], [0, .62], [-.08, .05]]
    target = [wrist - axis * .04, wrist, wrist + axis * .065]
    for y, along, width in [(.95, -.04, .049), (.62, 0, .048), (.3, .04, .057)]:
        for side in [-1, 1]:
            source.append([side * .35, y])
            target.append(wrist + axis * along + np.array([0, 0, side * width]))
    fingers = {
        'Pinky': [(-.55, -.39), (-.62, -.68)],
        'Ring': [(-.34, -.46), (-.37, -.84)],
        'Middle': [(-.055, -.48), (-.065, -.91)],
        'Index': [(.265, -.43), (.275, -.87)],
        'Thumb': [(.47, .08), (.62, -.285)],
    }
    for finger, coordinates in fingers.items():
        source.extend(coordinates)
        target.extend([BONES[f'RightHand{finger}1'], BONES[f'RightHand{finger}2'] * .7 + BONES[f'RightHand{finger}3'] * .3])
    warp = RBFInterpolator(np.array(source), np.array(target), kernel='thin_plate_spline', smoothing=.0001)
    out = warp(p[:, :2])
    normal = io.unit(np.array([-.88, .47, 0]))
    if not volume:
        return out + p[:, 2, None] * normal * .2
    levels = [-.95, -.65, -.45, -.2, .1, .4, .65, .95]
    center = np.interp(p[:, 1], levels, [.02, .03, .05, .024, -.021, -.039, 0, .015])
    radius = np.interp(p[:, 1], levels, [.135, .10, .09, .11, .10, .066, .105, .12])
    source_centers = p.copy()
    source_centers[:, 2] = 0
    lo, hi = ray_bounds(source_centers, np.array([0, 0, 1]), glove_triangles(), 1.)
    hit = np.isfinite(lo)
    center[hit], radius[hit] = (hi[hit] + lo[hit]) / 2, (hi[hit] - lo[hit]) / 2
    points, faces, _ = body_surface(('hands', 'forearms'), -1)
    lo, hi = ray_bounds(out, normal, points[faces], .09)
    hit = np.isfinite(lo)
    body_center = np.zeros(len(p))
    thickness = np.where(p[:, 1] < -.45, .016, .027)
    body_center[hit], thickness[hit] = (hi[hit] + lo[hit]) / 2, (hi[hit] - lo[hit]) / 2 + .007
    offset = np.clip((p[:, 2] - center) / radius, -1.3, 1.3) * thickness
    out += (offset + body_center)[:, None] * normal
    return out


def restrict_fingers(source, positions, joints, weights):
    dense = np.zeros((len(source), len(NAMES)))
    np.add.at(dense, (np.arange(len(source))[:, None], joints), weights)
    finger_ids = np.digitize(source[:, 0], [-.46, -.20, .13])
    for index, finger in enumerate(['Pinky', 'Ring', 'Middle', 'Index', 'Thumb']):
        mask = ((source[:, 1] < -.50) & (finger_ids == index) if finger != 'Thumb'
                else (source[:, 0] > .42) & (source[:, 1] < .15))
        allowed = [NAMES.index('RightHand')] + [NAMES.index(f'RightHand{finger}{i}') for i in range(1, 5)]
        rows = np.where(mask)[0]
        forbidden = [i for i in range(len(NAMES)) if i not in allowed]
        dense[np.ix_(rows, forbidden)] = 0
        missing = rows[dense[rows].sum(1) < 1e-8]
        if len(missing):
            start, end = BONES[f'RightHand{finger}1'], BONES[f'RightHand{finger}2']
            t = np.clip((positions[missing] - start) @ (end - start) / np.sum((end - start) ** 2), 0, 1)
            dense[missing, allowed[1]], dense[missing, allowed[2]] = 1 - t, t
        if len(rows):
            assert np.max(dense[np.ix_(rows, forbidden)]) == 0
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(1, keepdims=True)


def with_rig(doc, binary, name, version):
    indices = [i for i, node in enumerate(io.BASE['nodes']) if 'mesh' not in node]
    remap = {old: new for new, old in enumerate(indices)}
    doc['nodes'] = [copy.deepcopy(io.BASE['nodes'][i]) for i in indices]
    for node in doc['nodes']:
        if 'children' in node:
            node['children'] = [remap[i] for i in node['children'] if i in remap]
    skin = copy.deepcopy(io.BASE['skins'][0])
    skin['joints'] = [remap[i] for i in skin['joints']]
    if 'skeleton' in skin:
        skin['skeleton'] = remap[skin['skeleton']]
    skin['inverseBindMatrices'] = io.add_accessor(doc, binary, io.accessor(io.BASE, io.BASE_BIN, skin['inverseBindMatrices']), 'MAT4')
    doc['skins'] = [skin]
    doc['scenes'] = copy.deepcopy(io.BASE['scenes'])
    for scene in doc['scenes']:
        scene['nodes'] = [remap[i] for i in scene['nodes'] if i in remap]
    for index, mesh in enumerate(doc['meshes']):
        metadata = dict(rig_id='human_male_01_mixamo_candidate_v2', part_id=name, region=mesh['name'], fitting_status='candidate_' + version)
        mesh['extras'] = metadata
        doc['scenes'][0]['nodes'].append(len(doc['nodes']))
        doc['nodes'].append(dict(name=mesh['name'], mesh=index, skin=0, extras=metadata))
    doc.pop('animations', None)


def append_source(destination, binary, source, raw):
    offsets = {key: len(destination.setdefault(key, [])) for key in ['bufferViews', 'images', 'samplers', 'textures', 'materials']}
    binary.extend(b'\0' * (-len(binary) % 4))
    byte_offset = len(binary)
    binary.extend(raw)
    for view in source.get('bufferViews', []):
        view = copy.deepcopy(view)
        view['byteOffset'] = view.get('byteOffset', 0) + byte_offset
        destination['bufferViews'].append(view)
    for im in source.get('images', []):
        im = copy.deepcopy(im)
        im['bufferView'] += offsets['bufferViews']
        destination['images'].append(im)
    destination['samplers'].extend(copy.deepcopy(source.get('samplers', [])))
    for tex in source.get('textures', []):
        tex = copy.deepcopy(tex)
        tex['source'] += offsets['images']
        if 'sampler' in tex:
            tex['sampler'] += offsets['samplers']
        destination['textures'].append(tex)
    def remap_texture(value):
        if isinstance(value, dict):
            for key, child in value.items():
                if key.endswith('Texture') and isinstance(child, dict) and 'index' in child:
                    child['index'] += offsets['textures']
                else:
                    remap_texture(child)
    for material in source.get('materials', []):
        material = copy.deepcopy(material)
        remap_texture(material)
        material['doubleSided'] = True
        destination['materials'].append(material)
    return offsets['materials']


def validate(path):
    doc, binary = read_glb(path)
    names = [doc['nodes'][i]['name'] for i in doc['skins'][0]['joints']]
    assert names == NAMES
    assert np.array_equal(io.accessor(doc, binary, doc['skins'][0]['inverseBindMatrices']), io.accessor(io.BASE, io.BASE_BIN, io.BASE['skins'][0]['inverseBindMatrices']))
    base_nodes = {node['name']: node for node in io.BASE['nodes'] if 'mesh' not in node}
    for node in doc['nodes']:
        if 'mesh' in node:
            continue
        base = base_nodes[node['name']]
        for key in ['matrix', 'translation', 'rotation', 'scale']:
            assert node.get(key) == base.get(key), (node['name'], key)
        assert [doc['nodes'][i]['name'] for i in node.get('children', [])] == [io.BASE['nodes'][i]['name'] for i in base.get('children', []) if 'mesh' not in io.BASE['nodes'][i]]
    triangles, max_error, active = 0, 0, set()
    for mesh in doc['meshes']:
        for prim in mesh['primitives']:
            attrs = prim['attributes']
            p, j, w = [io.accessor(doc, binary, attrs[k]) for k in ['POSITION', 'JOINTS_0', 'WEIGHTS_0']]
            for accessor in attrs.values():
                assert np.isfinite(io.accessor(doc, binary, accessor)).all()
            assert np.min(j) >= 0 and np.max(j) < len(NAMES) and np.min(w) >= 0
            max_error = max(max_error, float(abs(w.sum(1) - 1).max()))
            assert max_error < 1e-6
            f = io.accessor(doc, binary, prim['indices']).reshape(-1, 3)
            assert f.max() < len(p)
            assert np.min(np.linalg.norm(np.cross(p[f[:, 1]] - p[f[:, 0]], p[f[:, 2]] - p[f[:, 0]]), axis=1)) > 1e-12
            triangles += len(f)
            active.update(NAMES[i] for i in j[w > 1e-6])
            if mesh['name'] == 'glove_rogue_right':
                assert all(NAMES[i].startswith('Right') for i in j[w > 1e-6])
            if mesh['name'] == 'wrap_rogue_left':
                assert all(NAMES[i] in ['LeftArm', 'LeftForeArm', 'LeftHand', 'LeftHandThumb1', 'LeftHandThumb2'] for i in j[w > 1e-6])
            if mesh['name'].startswith('shirt_sleeve_'):
                side = mesh['name'].rsplit('_', 1)[1].capitalize()
                assert all(NAMES[i] in ['Spine', 'Spine1', 'Spine2', 'Neck'] + [side + bone for bone in ['Shoulder', 'Arm', 'ForeArm']] for i in j[w > 1e-6])
    return dict(path=str(path.relative_to(ROOT)), sha256=digest(path), triangles=triangles, bones=65,
                exact_rest_hierarchy_and_inverse_bind_match=True, maximum_weight_sum_error=max_error, active_bones=sorted(active))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=OUTPUT)
    parser.add_argument('--report', type=Path, default=REPORT)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.report = args.report.resolve()
    selection = SELECTION
    assert digest(ROOT / selection['reference']['base']) == selection['reference']['base_sha256']
    review = json.loads((ROOT / 'doc/assets/modular-rogue-mesh-review-selected.json').read_text())
    hashes = {part['id']: part['sha256'] for part in review['parts']}
    args.output.mkdir(parents=True, exist_ok=True)
    specs = [
        ('top_rogue', ['top_rogue'], None, (), 0),
        ('pants_rogue', ['pants_rogue'], pants_fit, ('legs', 'ankles', 'torso'), 0),
        ('gloves_rogue', ['glove_rogue_right', 'wrap_rogue_left'], None, (), 0),
        ('boots_rogue', ['boot_rogue_left'], boot_fit, ('feet', 'ankles', 'legs'), 1),
    ]
    report = dict(status='fitting and skinning candidate; runtime acceptance pending', reference=selection['reference'],
                  generator='tools/fit-modular-rogue.py', uv_repairs={}, sources=[], outputs=[])
    version = args.output.name.rsplit('_', 1)[-1]
    for name, ids, fit, regions, side in specs:
        selected = selection['fitting_candidate'].get('part_overrides', {}).get(name)
        if selected:
            report['outputs'].append(validate(ROOT / selected))
            report['sources'].append(dict(id=name, path=selected, sha256=digest(ROOT / selected), role='User-selected fitted part'))
            continue
        doc = dict(asset={'version': '2.0', 'generator': 'OpenMMO rogue fitting ' + version}, accessors=[], bufferViews=[], meshes=[])
        binary = bytearray()
        for part_id in ids:
            part = next(p for p in selection['parts'] if p['id'] == part_id)
            path = ROOT / part['source_glb']
            assert digest(path) == hashes[part_id], f'Source changed: {path}'
            report['sources'].append(dict(id=part_id, path=part['source_glb'], sha256=hashes[part_id],
                role='design reference; geometry replaced in v5' if part_id in ['top_rogue', 'wrap_rogue_left'] else 'fitted source geometry'))
            if part_id == 'top_rogue':
                report['layer_rebuild'] = build_top(sys.modules[__name__], doc, binary)
                report['sleeve_connections'] = report['layer_rebuild']['sleeves']
                continue
            if part_id == 'wrap_rogue_left':
                report['wrist_wrap'] = build_wrap(sys.modules[__name__], doc, binary)
                continue
            src, raw = read_glb(path)
            assert len(src['nodes']) == 1 and not any(k in src['nodes'][0] for k in ['matrix', 'translation', 'rotation', 'scale'])
            offset = append_source(doc, binary, src, raw)
            for primitive in src['meshes'][0]['primitives']:
                a = primitive['attributes']
                p = io.accessor(src, raw, a['POSITION'])
                uv = io.accessor(src, raw, a['TEXCOORD_0'])
                f = io.accessor(src, raw, primitive['indices']).reshape(-1, 3)
                p, uv, f, repaired, vertex_map = repair_uv(p, uv, f)
                report['uv_repairs'][part_id] = repaired
                if part_id == 'glove_rogue_right':
                    fit, regions, side = glove_fit, ('hands', 'forearms'), -1
                positions = fit(p)
                normal_fit = (lambda q: glove_fit(q, volume=False)) if part_id == 'glove_rogue_right' else fit
                normals = io.transformed_normals(p, io.accessor(src, raw, a['NORMAL'])[vertex_map], normal_fit)
                allowed = set(NAMES)
                if name == 'pants_rogue':
                    allowed = {n for n in NAMES if n in ['Hips', 'Spine'] or any(n == s + b for s in ['Left', 'Right'] for b in ['UpLeg', 'Leg', 'Foot'])}
                elif part_id == 'glove_rogue_right':
                    allowed = {n for n in NAMES if n.startswith('RightHand') or n == 'RightForeArm'}
                elif name == 'boots_rogue':
                    allowed = {'LeftLeg', 'LeftFoot', 'LeftToeBase', 'LeftToe_End'}
                joints, weights = transfer(positions, body_surface(regions, side), allowed)
                if part_id == 'glove_rogue_right':
                    joints, weights = restrict_fingers(p, positions, joints, weights)
                if name == 'boots_rogue':
                    mirrored = joints.copy()
                    for i, joint_name in enumerate(NAMES):
                        if joint_name.startswith('Left'):
                            mirrored[joints == i] = NAMES.index(joint_name.replace('Left', 'Right', 1))
                    f = np.vstack([f, f[:, ::-1] + len(positions)])
                    right = boot_fit(p, 'Right')
                    right_normals = io.transformed_normals(p, io.accessor(src, raw, a['NORMAL'])[vertex_map], lambda q: boot_fit(q, 'Right'))
                    positions = np.vstack([positions, right])
                    normals = np.vstack([normals, right_normals])
                    uv = np.vstack([uv, uv])
                    joints = np.vstack([joints, mirrored])
                    weights = np.vstack([weights, weights])
                if name in ['pants_rogue', 'boots_rogue']:
                    joints, weights = ankle_weights(positions, joints, weights)
                attrs = io.add_skin_attributes(doc, binary, positions, normals, joints, weights)
                attrs['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
                attrs['TANGENT'] = io.add_accessor(doc, binary, io.tangents(positions, normals, uv, f), 'VEC4')
                doc['meshes'].append(dict(name=part_id, primitives=[dict(attributes=attrs, material=offset + primitive.get('material', 0), indices=io.add_accessor(doc, binary, f.reshape(-1, 1), 'SCALAR', 5125))]))
        if name in ['pants_rogue', 'boots_rogue']:
            add_ankle_lining(doc, binary, name)
        with_rig(doc, binary, name, version)
        path = args.output / f'{name}.glb'
        write_glb(path, doc, io.compact(doc, binary))
        result = validate(path)
        report['outputs'].append(result)
        print(name, result['triangles'], 'triangles; rig and weights passed')
    total = sum(o['triangles'] for o in report['outputs'])
    report['budget'] = dict(equipment_triangles=total, assembled_including_hidden_triangles=total + 13891 + 933 + 302, face_triangles=1505, runtime_visible_triangles=None)
    report['hand_influences'] = dict(right_only=True, left_wrap_matches_local_wrist_including_thumb_root=True, distal_finger_regions_restricted_to_own_finger=True)
    report['ankle_fitting'] = dict(interface_sha256=digest(ROOT / selection['reference']['interfaces']),
        method='Both cuffs follow the body section in its local plane, with trouser-inside-boot clearance and shared Leg/Foot weights',
        verified_overlap_interval_m=[-.030, 0], added_lining_triangles=320)
    report['ankle_connections'] = [dict(side=side, interface='shoe_ankle_' + side, center=ankle_interface(side)[0].tolist(), axis=ankle_interface(side)[1].tolist(), basis=ankle_interface(side)[2].tolist(), weights={NAMES[i]: float(w) for i, w in enumerate(ankle_interface(side)[4]) if w > 0}, sample_heights_m=[-.030, -.025, -.020, -.015, -.010, -.005, 0]) for side in ['Left', 'Right']]
    report['pending'] = ['Right glove finger openings retain source geometry; final hand equipment acceptance pending', 'Long-glove clearance and occlusion with actual mixed equipment', 'Barefoot trouser variant and connections to other boot sets', 'Full gameplay and mixed outfit acceptance beyond the reviewed set']
    args.report.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
