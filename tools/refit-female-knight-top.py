"""Fit the archived Tripo plate as a shell, keeping the existing sleeve interfaces."""
import hashlib
from io import BytesIO
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.sparse import coo_matrix, diags
from scipy.sparse.csgraph import connected_components
from scipy.sparse.linalg import spsolve

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.plate_fitting import plate_roughness_map, similarity
from outfits.rogue_layers import compact_weights

ROOT = Path(__file__).resolve().parents[1]
ITEM = ROOT / 'assets/items/female_knight_top/tripo_v1'


def unit(v):
    return v / np.maximum(np.linalg.norm(v, axis=1, keepdims=True), 1e-12)


def fit_regions(points, faces, uv, metal_map, left, upper_arm):
    samples = [uv[faces].mean(1)]
    for corner in range(3):
        samples.append(.55 * uv[faces[:, corner]] + .45 * samples[0])
    x, y = np.moveaxis(np.asarray(samples) % 1, -1, 0)
    steel = metal_map[(y * metal_map.shape[0]).astype(int), (x * metal_map.shape[1]).astype(int)].mean(0) > 80
    welded, first, inverse = np.unique(points.round(6), axis=0, return_index=True, return_inverse=True)
    triangles = inverse[faces]
    metal = triangles[steel]
    graph = coo_matrix((np.ones(metal.size), (metal.ravel(), np.roll(metal, 1, axis=1).ravel())),
                       shape=(len(welded), len(welded)))
    _, islands = connected_components(graph, directed=False)
    counts = np.bincount(islands[metal[:, 0]], minlength=len(welded))
    panels = np.flatnonzero(counts > 300)
    assert len(panels) == 3, 'Expected the cuirass and two pauldrons in the original ORM'
    labels = np.full(len(welded), -1)
    labels[counts[islands] >= 5] = 0
    for island in panels:
        vertices = islands == island
        if upper_arm[first[vertices]].mean() > .4:
            labels[vertices] = 1 if left[first[vertices]].mean() > .5 else 2
    cuffs = (labels < 0) & (welded[:, 1] < -.24) & (upper_arm[first] > .9)
    labels[cuffs] = np.where(left[first[cuffs]], 3, 4)
    field = np.zeros((len(welded), 5))
    fixed = np.flatnonzero(labels >= 0)
    free = np.flatnonzero(labels < 0)
    field[fixed, labels[fixed]] = 1
    # Interpolate part transforms only in the leather and textile between the rigid surfaces.
    edges = np.unique(np.sort(triangles[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2), axis=1), axis=0)
    a, b = edges.T
    conductance = 1 / np.maximum(np.linalg.norm(welded[a] - welded[b], axis=1), 1e-6)
    neighbors = coo_matrix((np.tile(conductance, 2), (np.r_[a, b], np.r_[b, a])), shape=graph.shape).tocsr()
    laplacian = diags(np.asarray(neighbors.sum(1)).ravel()) - neighbors
    field[free] = spsolve(laplacian[free][:, free].tocsc(), -laplacian[free][:, fixed] @ field[fixed])
    return field[inverse], labels[inverse], faces[steel][counts[islands[metal[:, 0]]] >= 5]


def lower_front_rim(position, normal):
    def drop(p):
        x, y, z = p.T
        across = np.clip((x + 0.0008) / 0.067, -1, 1)
        arch = (1 + np.cos(np.pi * across)) * 0.5
        front = np.clip((z + 0.008) / 0.038, 0, 1)
        height = np.clip((y - 1.547) / 0.036, 0, 1)
        return 0.018 * arch * front * front * (3 - 2 * front) * height

    depth = drop(position)
    active = depth > 0
    p = position[active]
    axes = np.eye(3) * 0.00001
    slope = np.stack([(drop(p + axis) - drop(p - axis)) / 0.00002 for axis in axes], axis=1)
    n = normal[active]
    normal[active] = unit(n + slope * (n[:, 1] / (1 - slope[:, 1]))[:, None])
    position[:, 1] -= depth
    return active


def main():
    source, raw = read_glb(ITEM / 'source/model_url.glb')
    base, body_raw = read_glb(ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb')
    src = source['meshes'][0]['primitives'][0]
    a = src['attributes']
    points = accessor(source, raw, a['POSITION']).astype(float)
    normals = accessor(source, raw, a['NORMAL']).astype(float)
    with np.load(ITEM / 'fit-anchors.npz') as anchors:
        target, target_normals, joints, weights = (anchors[k] for k in ('POSITION', 'NORMAL', 'JOINTS_0', 'WEIGHTS_0'))
    skeleton = base['skins'][0]
    names = [base['nodes'][j]['name'] for j in skeleton['joints']]
    faces = accessor(source, raw, src['indices']).reshape(-1, 3)
    uv = accessor(source, raw, a['TEXCOORD_0'])
    orm = source['materials'][0]['pbrMetallicRoughness']['metallicRoughnessTexture']['index']
    image = source['images'][source['textures'][orm]['source']]
    metal_map = np.asarray(Image.open(BytesIO(view_bytes(source, raw, image['bufferView']))))[:, :, 2]
    trunk = np.isin(joints, [names.index('Spine1'), names.index('Spine2')])
    left = target[:, 0] > 0
    field, labels, steel = fit_regions(points, faces, uv, metal_map, left, (weights * ~trunk).sum(1))
    assert set(labels[steel].ravel()) <= {0, 1, 2}
    assert (np.ptp(labels[steel], axis=1) == 0).all()
    position, normal = np.zeros_like(points), np.zeros_like(normals)
    skin = np.zeros((len(points), len(names)))
    original_skin = np.zeros_like(skin)
    np.add.at(original_skin, (np.arange(len(points))[:, None], joints), weights)
    scales = []
    cuff = field[:, 3:].sum(axis=1, keepdims=True)
    position += cuff * target
    normal += cuff * target_normals
    skin += cuff * original_skin
    for region, bone in enumerate(['Spine1', 'LeftArm', 'RightArm']):
        anchors = labels == region
        rotation, scale, shift = similarity(points[anchors], target[anchors])
        influence = field[:, region, None]
        position += influence * (points @ rotation * scale + shift)
        normal += influence * (normals @ rotation)
        skin[:, names.index(bone)] += influence[:, 0]
        scales.append(scale)
    normal = unit(normal)
    selected, new_weights = compact_weights(skin.clip(0, 1))
    selected[new_weights == 0] = 0
    collar = lower_front_rim(position, normal)
    binary = bytearray()
    part = 'top_female_knight'
    fitted = dict(asset=source['asset'], scene=base['scene'], scenes=base['scenes'], nodes=list(base['nodes']),
                  skins=[dict(skeleton)], accessors=[], bufferViews=[], meshes=[dict(name=part)],
                  materials=source['materials'], textures=source['textures'], images=source['images'])
    fitted['samplers'] = [dict(magFilter=9729, minFilter=9987)]
    for texture in fitted['textures']:
        texture['sampler'] = 0
    fitted['nodes'][0] = dict(name=part, skin=0, mesh=0, extras=dict(rig_id='human_female_01_local_v1', part_id=part, fitting_version=4))
    attributes = {key: add_accessor(fitted, binary, values, kind, component)
                  for key, values, kind, component in [
                      ('POSITION', position, 'VEC3', 5126), ('NORMAL', normal, 'VEC3', 5126),
                      ('TEXCOORD_0', uv, 'VEC2', 5126), ('JOINTS_0', selected, 'VEC4', 5123),
                      ('WEIGHTS_0', new_weights, 'VEC4', 5126)]}
    indices = add_accessor(fitted, binary, faces.ravel(), 'SCALAR', 5123)
    fitted['meshes'][0]['primitives'] = [dict(attributes=attributes, indices=indices, material=0)]
    bind = accessor(base, body_raw, skeleton['inverseBindMatrices'])
    fitted['skins'][0]['inverseBindMatrices'] = add_accessor(fitted, binary, bind, 'MAT4')
    fitted['materials'][0]['normalTexture']['scale'] = 0.5
    for texture in fitted['images']:
        data = view_bytes(source, raw, texture['bufferView'])
        if texture is image:
            data = plate_roughness_map(data)
            texture['mimeType'] = 'image/png'
        binary.extend(bytes(-len(binary) % 4))
        texture['bufferView'] = len(fitted['bufferViews'])
        fitted['bufferViews'].append(dict(buffer=0, byteOffset=len(binary), byteLength=len(data)))
        binary.extend(data)
    fitted['buffers'] = [dict(byteLength=len(binary))]
    out = ITEM / 'fitted_v4'
    out.mkdir(exist_ok=True)
    write_glb(out / 'top_female_knight.glb', fitted, binary)
    edges = steel[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)
    lengths = np.linalg.norm(points[edges[:, 0]] - points[edges[:, 1]], axis=1)
    expected = lengths * np.asarray(scales)[labels[edges[:, 0]]]

    def edge_error(shape):
        distances = np.linalg.norm(shape[edges[:, 0]] - shape[edges[:, 1]], axis=1)
        return abs(distances / expected - 1)

    error = edge_error(position.astype('<f4'))
    unchanged = ~collar[edges].any(axis=1)
    assert error[unchanged].max() < .0001
    report = {
        'archived_tripo_sha256': hashlib.sha256((ITEM / 'source/model_url.glb').read_bytes()).hexdigest(),
        'uniform_scales': dict(zip(['cuirass', 'left_pauldron', 'right_pauldron'], scales)),
        'steel_triangles': len(steel),
        'total_triangles': len(faces),
        'edge_length_check': 'Compared with the archived Tripo edges after one similarity transform per panel.',
        'maximum_relative_error_outside_collar': float(error[unchanged].max()),
        'front_collar_drop_mm': 18,
        'reshaped_steel_triangles': int(collar[steel].any(axis=1).sum()),
        'previous_fit_relative_error_p95': float(np.percentile(edge_error(target), 95)),
    }
    (ROOT / 'doc/assets/modular-female-knight-top-fitting-v4.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))



if __name__ == '__main__':
    main()
