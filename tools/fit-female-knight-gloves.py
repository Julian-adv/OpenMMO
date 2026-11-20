"""Fit the source gauntlet to the female rig, then adapt its mirrored copy."""
import argparse
import copy
import hashlib
import importlib.util
import json
import subprocess
from pathlib import Path

import numpy as np
from scipy.sparse import coo_matrix, diags
from scipy.sparse.csgraph import connected_components, dijkstra
from scipy.sparse.linalg import spsolve

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.knight_steel import match_gauntlet_steel
from outfits.rogue_layers import compact_weights

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'assets/items/female_knight_gloves/tripo_v1'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
SOURCE = DIRECTORY / 'source/model_url.glb'
OUT = DIRECTORY / 'fitted_v3'
spec = importlib.util.spec_from_file_location('surface_tools', ROOT / 'tools/fit-modular-rogue.py')
surface_tools = importlib.util.module_from_spec(spec)
spec.loader.exec_module(surface_tools)
io = surface_tools.io
FINGERS = ['Pinky', 'Ring', 'Middle', 'Index', 'Thumb']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def smooth(value):
    t = np.clip(value, 0, 1)
    return t * t * (3 - 2 * t)


def setup_body():
    doc, raw = read_glb(BODY)
    skin = doc['skins'][0]
    names = [doc['nodes'][i]['name'] for i in skin['joints']]
    bind = accessor(doc, raw, skin['inverseBindMatrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
    bones = dict(zip(names, np.linalg.inv(bind)[:, :3, 3]))
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    points = accessor(doc, raw, attrs['POSITION']).astype(float)
    faces = accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    joints = accessor(doc, raw, attrs['JOINTS_0'])
    weights = accessor(doc, raw, attrs['WEIGHTS_0'])
    dense = np.zeros((len(points), len(names)))
    np.add.at(dense, (np.arange(len(points))[:, None], joints), weights)
    return doc, raw, names, bones, (points, faces, dense)


def finger_labels(points, faces):
    vertices, inverse = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    welded = inverse[faces]
    distal = welded[(vertices[welded, 1] < -.34).all(1)]
    edges = distal[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)
    _, groups = connected_components(coo_matrix((np.ones(len(edges)), (edges[:, 0], edges[:, 1])), shape=(len(vertices),)*2), directed=False)
    seeds = [np.where(groups == label)[0] for label in np.unique(groups) if np.sum(groups == label) > 4]
    seeds.sort(key=lambda rows: vertices[rows, 2].mean())
    assert len(seeds) == 5
    edges = np.unique(np.sort(welded[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2), axis=1), axis=0)
    lengths = np.linalg.norm(vertices[edges[:, 0]] - vertices[edges[:, 1]], axis=1)
    graph = coo_matrix((np.r_[lengths, lengths], (np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]])), shape=(len(vertices),)*2).tocsr()
    distances = np.array([dijkstra(graph, indices=rows, min_only=True) for rows in seeds])
    labels = distances.argmin(0)[inverse]
    active = points[:, 1] < -.285
    active |= (points[:, 2] > .103) & (points[:, 1] < -.235)
    return np.where(active, labels, -1)


def fit_side(points, faces, names, bones, body, side):
    wrist, elbow = bones[side + 'Hand'], bones[side + 'ForeArm']
    axis = io.unit(wrist - elbow)
    width = bones[side + 'HandIndex1'] - bones[side + 'HandPinky1']
    width = io.unit(width - axis * np.dot(width, axis))
    normal = io.unit(np.cross(axis, width))
    if side == 'Left':
        normal = -normal
    labels = finger_labels(points, faces)
    alignment = {}
    height = points[:, 1]
    distance = (-.145 - height) * .558
    distance += np.maximum(height - .32, 0) * .216
    radius = np.interp(height, [-.15, .10, .22, .32], [.432, .45, .477, .477])
    mapped = wrist + distance[:, None] * axis
    mapped += points[:, 2, None] * radius[:, None] * width
    mapped += points[:, 0, None] * radius[:, None] * normal
    palm = wrist + ((-.145-height)*.72)[:, None]*axis
    palm += points[:, 2, None]*.522*width
    palm += (points[:, 0]-.025-.40*points[:, 2])[:, None]*.414*normal
    blend = smooth((-.13-height)/.055)
    mapped = mapped*(1-blend[:, None])+palm*blend[:, None]
    for i, finger in enumerate(FINGERS):
        rows = np.where(labels == i)[0]
        own_points = points[rows]
        y_root = [-.295, -.298, -.296, -.285, -.241][i]
        y_tip = own_points[:, 1].min()
        fractions = np.array([0, .42, .75, 1])
        centers = []
        for fraction in fractions:
            y = y_root+(y_tip-y_root)*fraction
            near = own_points[np.argsort(abs(own_points[:, 1]-y))[:10]]
            center = (near.min(0)+near.max(0))/2
            center[1] = y
            centers.append(center)
        centers = np.array(centers)
        target_centers = np.array([bones[f'{side}Hand{finger}{j}'] for j in range(1,5)])
        target_centers[-1] += io.unit(target_centers[-1]-target_centers[-2])*.0036
        alignment[finger] = {"source_sections":centers.tolist(), "target_sections":target_centers.tolist()}
        t = (own_points[:, 1]-y_root)/(y_tip-y_root)
        source_center = np.column_stack([np.interp(t, fractions, centers[:, j]) for j in range(3)])
        target_center = np.column_stack([np.interp(t, fractions, target_centers[:, j]) for j in range(3)])
        target_axis = io.unit(target_centers[-1]-target_centers[0])
        target_width = io.unit(width-target_axis*np.dot(width,target_axis))
        target_normal = io.unit(np.cross(target_axis,target_width))
        if side == 'Left':
            target_normal = -target_normal
        cross = own_points-source_center
        fitted = target_center+cross[:,2,None]*.414*target_width+cross[:,0,None]*.396*target_normal
        amount = smooth((t+.08)/.35)
        mapped[rows] = mapped[rows]*(1-amount[:,None])+fitted*amount[:,None]
    own = np.array([name.startswith(side) and ('Hand' in name or 'Arm' in name) for name in names])
    own_faces = body[2][:, own].sum(1)[body[1]].mean(1) > .5
    surface = (body[0], body[1][own_faces], body[2])
    rest_points = wrist + ((-.145-height)*.558)[:, None]*axis + points[:,2,None]*.558*width + points[:,0,None]*.558*normal
    fixed = (height > -.175) | (height < -.37) | ((labels == 4) & (height < -.31))
    arap_path = OUT / f'arap-{side}.npz'
    np.savez(arap_path, points=rest_points, faces=faces, colors=np.zeros_like(points), fixed=fixed, targets=mapped)
    subprocess.run([str(ROOT / '.venv/bin/python'), str(ROOT / 'tools/outfits/fit_female_knight_top_surface.py'), str(arap_path)], check=True)
    mapped = np.load(OUT / 'arap-result.npz')['points']
    for iteration in range(3):
        for label in [-1, *range(5)]:
            rows = np.where(labels == label)[0]
            if not len(rows):
                continue
            if label < 0:
                selected = surface
            else:
                ids = [i for i, name in enumerate(names) if name.startswith(side + 'Hand' + FINGERS[label])]
                finger_faces = body[2][:, ids].sum(1)[body[1]].mean(1) > .20
                selected = (body[0], body[1][finger_faces], body[2])
            closest, outward, _ = surface_tools.nearest_surface(mapped[rows], selected)
            signed = np.sum((mapped[rows] - closest) * outward, axis=1)
            clearance = np.where(points[rows, 1] > .19, .0144, np.where(points[rows, 1] > -.14, .0072, .00288))
            push = np.maximum(clearance - signed, 0)
            mapped[rows] += push[:, None] * outward
    dense = np.zeros((len(points), len(names)))
    hand_blend = smooth((-.11-height)/.10)
    dense[:, names.index(side+'ForeArm')] = 1-hand_blend
    dense[:, names.index(side+'Hand')] = hand_blend
    for label, finger in enumerate(FINGERS):
        rows = labels == label
        y_root = [-.295, -.298, -.296, -.285, -.241][label]
        t = (height[rows]-y_root)/(height[rows].min()-y_root)
        first = smooth((t+.12)/.26)
        second = smooth((t-.32)/.20)
        third = smooth((t-.65)/.20)
        dense[rows] = 0
        for bone, values in [(side+'Hand',1-first), (f'{side}Hand{finger}1',first*(1-second)),
                             (f'{side}Hand{finger}2',first*second*(1-third)), (f'{side}Hand{finger}3',first*second*third)]:
            dense[rows, names.index(bone)] = values
    rigid = points[:, 1] > -.105
    dense[rigid] = 0
    dense[rigid, names.index(side + 'ForeArm')] = 1
    elbow_blend = .55 * smooth((height - .16) / .14)
    dense[:, names.index(side + 'ForeArm')] -= elbow_blend
    dense[:, names.index(side + 'Arm')] += elbow_blend
    unique, first, inverse = np.unique(np.round(points, 7), axis=0, return_index=True, return_inverse=True)
    edges = np.unique(np.sort(inverse[faces[:, [0,1,1,2,2,0]]].reshape(-1,2),axis=1),axis=0)
    a,b = edges.T
    strength = 1/np.maximum(np.linalg.norm(unique[a]-unique[b],axis=1),.001)
    adjacency = coo_matrix((np.r_[strength,strength],(np.r_[a,b],np.r_[b,a])),shape=(len(unique),)*2).tocsr()
    laplacian = diags(np.asarray(adjacency.sum(1)).ravel())-adjacency
    locked = (height > -.255) & (points[:,2] < .085)
    locked |= (height > -.20) | (height < -.36) | ((labels == 4) & (height < -.305))
    fixed, free = np.flatnonzero(locked[first]), np.flatnonzero(~locked[first])
    welded_weights = dense[first].copy()
    welded_weights[free] = spsolve(laplacian[free][:,free].tocsc(),-laplacian[free][:,fixed]@welded_weights[fixed])
    dense = np.maximum(welded_weights[inverse],0)
    empty = dense.sum(1) < 1e-8
    dense[empty, names.index(side + 'Hand')] = 1
    joints, weights = compact_weights(dense)
    for values in [mapped, weights]:
        assert np.isfinite(values).all()
    return mapped, joints, weights, labels, alignment


def attach_rig(doc, raw, base, base_raw, sides):
    ids = base['skins'][0]['joints']
    remap = {old: i for i, old in enumerate(ids)}
    doc['nodes'] = [copy.deepcopy(base['nodes'][i]) for i in ids]
    for bone in doc['nodes']:
        if 'children' in bone:
            bone['children'] = [remap[i] for i in bone['children'] if i in remap]
    children = {child for bone in doc['nodes'] for child in bone.get('children', [])}
    armature = len(doc['nodes'])
    doc['nodes'].append({'name':'Armature', 'children':[i for i in range(len(ids)) if i not in children], 'extras':{'rig_id':'human_female_01_local_v1'}})
    doc['scenes'] = [{'nodes':[armature], 'extras':{'rig_id':'human_female_01_local_v1'}}]
    doc['scene'] = 0
    skin = copy.deepcopy(base['skins'][0])
    skin['joints'] = [remap[i] for i in ids]
    skin['skeleton'] = remap[skin['skeleton']]
    skin['inverseBindMatrices'] = add_accessor(doc, raw, accessor(base, base_raw, skin['inverseBindMatrices']), 'MAT4')
    doc['skins'] = [skin]
    for i, side in enumerate(sides):
        doc['scenes'][0]['nodes'].append(len(doc['nodes']))
        doc['nodes'].append({'name':f'glove_female_knight_{side.lower()}', 'mesh':i, 'skin':0,
                             'extras':{'rig_id':'human_female_01_local_v1', 'part_id':'gloves_female_knight'}})
    doc.pop('animations', None)


def elbow_lining(doc, raw, body, names, bones, side, material):
    points, faces, dense = body
    axis = io.unit(bones[side + 'Hand'] - bones[side + 'ForeArm'])
    offset = points - bones[side + 'ForeArm']
    along = offset @ axis
    radius = np.linalg.norm(offset - along[:, None] * axis, axis=1)
    ids = [names.index(side + suffix) for suffix in ['Arm', 'ForeArm']]
    selected = (along > -.108) & (along < .063) & (radius < .099) & (dense[:, ids].sum(1) > .99)
    triangles = faces[selected[faces].all(1)]
    used, indices = np.unique(triangles, return_inverse=True)
    normals = io.smooth_normals(points, faces)[used]
    positions = points[used] + normals * .00144
    weights = np.zeros_like(dense[used])
    weights[:, ids] = dense[used][:, ids]
    joints, weights = compact_weights(weights)
    attrs = io.add_skin_attributes(doc, raw, positions, normals, joints, weights)
    return {'attributes': attrs, 'indices': add_accessor(doc, raw, indices.reshape(-1, 1), 'SCALAR', 5125), 'material': material}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--pair', action='store_true')
    args = parser.parse_args()
    OUT.mkdir(exist_ok=True)
    assert digest(SOURCE) == '8679c17277ec4fc73d683ffbbf07a00c970e550d537ed0514db80e5492815b57'
    assert digest(BODY) == '3eb7c0a5f3c6b30aec0fbae0b70c8fd925e4cb91772647420007eee795a1e0a5'
    base, base_raw, names, bones, body = setup_body()
    doc, source_raw = read_glb(SOURCE)
    primitive = doc['meshes'][0]['primitives'][0]
    points = accessor(doc, source_raw, primitive['attributes']['POSITION']).astype(float)
    uv = accessor(doc, source_raw, primitive['attributes']['TEXCOORD_0'])
    faces = accessor(doc, source_raw, primitive['indices']).reshape(-1, 3)
    source_triangles = points[faces]
    centers = source_triangles.mean(1)
    normals = io.unit(np.cross(source_triangles[:, 1]-source_triangles[:, 0], source_triangles[:, 2]-source_triangles[:, 0]))
    internal_lining = (centers[:, 1] > .24) & ((normals[:, 0]*centers[:, 0]+normals[:, 2]*centers[:, 2]) < -.01)
    faces = faces[~internal_lining]
    used, remapped = np.unique(faces, return_inverse=True)
    points, uv, faces = points[used], uv[used], remapped.reshape(-1, 3)
    raw = bytearray(source_raw)
    doc['meshes'] = []
    lining_material = len(doc['materials'])
    doc['materials'].append({'name': 'elbow_fabric_lining', 'pbrMetallicRoughness': {'baseColorFactor': [.009, .010, .012, 1], 'metallicFactor': 0, 'roughnessFactor': .95}, 'doubleSided': True})
    sides = ['Right', 'Left'] if args.pair else ['Right']
    records, arrays = {}, {}
    for side in sides:
        mapped, joints, weights, labels, alignment = fit_side(points, faces, names, bones, body, side)
        triangles = faces[:, ::-1] if side == 'Right' else faces
        normals = io.smooth_normals(mapped, triangles)
        attrs = io.add_skin_attributes(doc, raw, mapped, normals, joints, weights)
        attrs['TEXCOORD_0'] = add_accessor(doc, raw, uv, 'VEC2')
        attrs['TANGENT'] = add_accessor(doc, raw, io.tangents(mapped, normals, uv, triangles), 'VEC4')
        doc['meshes'].append({'name':f'glove_female_knight_{side.lower()}', 'primitives':[{'attributes':attrs, 'indices':add_accessor(doc, raw, triangles.reshape(-1, 1), 'SCALAR', 5125), 'material':primitive['material']}]})
        lining = elbow_lining(doc, raw, body, names, bones, side, lining_material)
        doc['meshes'][-1]['primitives'].append(lining)
        arrays.update({f'{side}_positions':mapped, f'{side}_joints':joints, f'{side}_weights':weights, f'{side}_labels':labels})
        records[side] = {'triangles':len(triangles), 'lining_triangles': doc['accessors'][lining['indices']]['count'] // 3, 'vertices':len(mapped), 'alignment':alignment}
    attach_rig(doc, raw, base, base_raw, sides)
    match_gauntlet_steel(doc, raw)
    raw = io.compact(doc, raw)
    target = OUT / ('gloves_female_knight.glb' if args.pair else 'glove_right.glb')
    write_glb(target, doc, raw)
    np.savez(OUT / 'fit-data.npz', source=points, faces=faces, bone_names=names, **arrays)
    hide = np.zeros(len(body[1]), dtype=bool)
    for side in sides:
        wrist, elbow = bones[side + 'Hand'], bones[side + 'ForeArm']
        axis = io.unit(wrist - elbow)
        along = (body[0] - elbow) @ axis
        radial = np.linalg.norm(body[0] - elbow - along[:, None] * axis, axis=1)
        sign = -1 if side == 'Right' else 1
        covered = (along > -.0072) & (radial < .1296) & (body[0][:, 0] * sign > .234)
        hide |= covered[body[1]].all(1)
    mask = {'body':str(BODY.relative_to(ROOT)), 'body_sha256':digest(BODY), 'mesh_index':0, 'primitive_index':0,
            'primitive_triangles':len(body[1]), 'source_triangles':sum(base['accessors'][pr['indices']]['count']//3 for pr in base['meshes'][0]['primitives']), 'hide_triangle_indices':np.flatnonzero(hide).tolist()}
    (OUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2)+'\n')
    top_path = ROOT / 'assets/items/female_knight_top/tripo_v1/fitted_v4/top_female_knight.glb'
    top, top_raw = read_glb(top_path)
    top_primitive = top['meshes'][0]['primitives'][0]
    with np.load(top_path.parents[1] / 'fit-anchors.npz') as anchors:
        top_points = anchors['POSITION']
    top_faces = accessor(top, top_raw, top_primitive['indices']).reshape(-1, 3)
    covered = np.zeros(len(top_points), dtype=bool)
    for side in sides:
        elbow, wrist = bones[side+'ForeArm'], bones[side+'Hand']
        axis = io.unit(wrist-elbow)
        offset = top_points-elbow
        along = offset@axis
        radius = np.linalg.norm(offset-along[:,None]*axis, axis=1)
        covered |= (along > -.045) & (radius < .126)
    sleeve_mask = {'source_sha256':digest(top_path), 'mesh_index':0, 'primitive_index':0, 'primitive_triangles':len(top_faces), 'hide_triangle_indices':np.flatnonzero(covered[top_faces].all(1)).tolist()}
    (OUT / 'top-occlusion.json').write_text(json.dumps(sleeve_mask, indent=2)+'\n')
    report = {'date':'2026-10-14', 'source_sha256':digest(SOURCE), 'body_sha256':digest(BODY), 'output_sha256':digest(target),
              'status':'fitting candidate; pending visual and animation review', 'sides':records,
              'method':'Geodesic digit regions and source sections map to canonical finger bones; ARAP palm transition and measured rest clearance. Harmonic weights join palm to finger chains, with the forearm plate rigid. Mirrored design adapts to actual left-hand rest bones.',
              'steel_finish':'fitted_v4 cuirass', 'body_hidden_triangles':int(hide.sum()),
              'removed_internal_lining_triangles':int(internal_lining.sum()), 'top_hidden_triangles':len(sleeve_mask['hide_triangle_indices'])}
    (OUT / 'fitting.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({'output':str(target), 'sides':sides, 'triangles':sum(record['triangles'] + record['lining_triangles'] for record in records.values())}))


if __name__ == '__main__':
    main()
