"""Fit the supplied mitre to the canonical head and priest collar."""
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components

from lib.glb import view_bytes

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/priest/tripo_helmet_v1'
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io


def main():
    source = OUTPUT / 'source.glb'
    assert fit.digest(source) == 'c67ff9ff51ff8a6f44193cd9d93dae6d70c446ebd4be11bfdc92afa0006c831f'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(doc, raw, attrs['POSITION'])
    uv = io.accessor(doc, raw, attrs['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    scale, offset = [.43, .465, .44], [0, 1.628, -.003]
    fitted = points * scale + offset
    lappets = points[:, 1] < .39
    fitted[lappets, 2] -= .04 * io.smoothstep((1.81 - fitted[lappets, 1]) / .09)
    head = fit.body_surface(('head',))
    for _ in range(8):
        nearest, normal, _ = fit.nearest_surface(fitted, head)
        signed = np.einsum('ij,ij->i', fitted - nearest, normal)
        active = (fitted[:, 1] > 1.79) & (np.linalg.norm(fitted - nearest, axis=1) < .035) & (signed < .003)
        fitted[active] = nearest[active] + normal[active] * .003
        samples = fitted[faces].mean(1)
        nearest, normal, _ = fit.nearest_surface(samples, head)
        signed = np.einsum('ij,ij->i', samples - nearest, normal)
        active = (samples[:, 1] > 1.79) & (np.linalg.norm(samples - nearest, axis=1) < .035) & (signed < .002)
        updates, counts = np.zeros_like(fitted), np.zeros(len(fitted))
        for corner in range(3):
            np.add.at(updates, faces[active, corner], (nearest + normal * .003 - samples)[active])
            np.add.at(counts, faces[active, corner], 1)
        moved = counts > 0
        fitted[moved] += updates[moved] / counts[moved, None]
    joints = np.tile([fit.NAMES.index(name) for name in ['Head', 'Neck', 'Spine2']] + [0], (len(fitted), 1)).astype(np.uint16)
    weights = np.zeros((len(fitted), 4))
    head_weight = np.where(lappets, io.smoothstep((fitted[:, 1] - 1.65) / .14), 1)
    neck_weight = (1 - head_weight) * io.smoothstep((fitted[:, 1] - 1.60) / .12)
    weights[:, 0], weights[:, 1], weights[:, 2] = head_weight, neck_weight, 1 - head_weight - neck_weight
    normals = io.smooth_normals(fitted, faces)
    welded, remap = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    welded_faces = remap[faces]
    edges = np.concatenate([welded_faces[:, pair] for pair in [[0, 1], [1, 2], [2, 0]]])
    _, components = connected_components(coo_matrix((np.ones(len(edges)), (edges[:, 0], edges[:, 1])),
        shape=(len(welded), len(welded))), directed=False)
    shell = np.argmax(np.bincount(components))
    triangles = points[faces]
    centers = triangles.mean(1)
    face_normals = io.unit(np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]))
    inward = np.einsum('ij,ij->i', face_normals[:, [0, 2]], centers[:, [0, 2]]) < -.025
    band = inward & (centers[:, 1] > .40) & (centers[:, 1] < .465)
    band &= components[welded_faces[:, 0]] == shell
    samples = fitted[faces[band]].mean(1)
    nearest, normal, _ = fit.nearest_surface(samples, head)
    gaps = np.einsum('ij,ij->i', samples - nearest, normal)
    assert gaps.min() > 0, 'Head intersects an inner band triangle center'
    band_clearance = dict(samples=len(samples), minimum_mm=float(gaps.min() * 1000),
        median_mm=float(np.median(gaps) * 1000), maximum_mm=float(gaps.max() * 1000),
        method='Signed nearest-head distances at inner band triangle centers; 3mm vertex and 2mm triangle targets')
    inward &= (centers[:, 1] > .47) & (components[welded_faces[:, 0]] == shell)
    lining_vertices, lining_faces = np.unique(faces[inward], return_inverse=True)
    faces[inward] = lining_faces.reshape(-1, 3) + len(points)
    fitted = np.vstack([fitted, fitted[lining_vertices]])
    normals = np.vstack([normals, normals[lining_vertices]])
    joints = np.vstack([joints, joints[lining_vertices]])
    weights = np.vstack([weights, weights[lining_vertices]])
    original_uv = uv.copy()
    uv = np.vstack([uv, uv[lining_vertices] * .04 + [.72, .38]]).astype(np.float32)
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    binary = bytearray(raw)
    skin = io.add_skin_attributes(doc, binary, fitted, normals, joints, weights)
    skin['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    doc['meshes'] = [dict(name='helmet_priest', primitives=[dict(attributes=skin,
        material=primitive['material'], indices=io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125))])]
    fit.with_rig(doc, binary, 'helmet_priest', 'tripo_priest_helmet_v2')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'helmet_priest.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    assert np.array_equal(original_uv, uv[:len(original_uv)])
    assert np.array_equal(uv, io.accessor(doc, binary, doc['meshes'][0]['primitives'][0]['attributes']['TEXCOORD_0']))
    report = dict(date='2026-10-09', source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source),
        original_path=r'Y:\public\web_downloads\bishop+mitre+3d+model.glb', mounted_path='/mnt/y/web_downloads/bishop+mitre+3d+model.glb',
        provider='User-supplied Tripo output', received_date='2026-10-09', generation_date=None,
        subscription_tier='Unconfirmed for this file', license='Original Tripo output terms; see doc/assets/characters.md'),
        base=dict(path=str((io.PARTS / 'fitted/base.glb').relative_to(ROOT)), sha256=fit.digest(io.PARTS / 'fitted/base.glb')),
        output=dict(path=str(target.relative_to(ROOT)), sha256=fit.digest(target)),
        initial_transform=dict(scale=scale, translation_m=offset), inner_band_clearance=band_clearance, source_triangles=len(faces), fitted_triangles=len(faces),
        texture_bytes_preserved=True, exterior_uv_preserved=True, triangle_count_preserved=True, material_preserved=True,
        lining=dict(remapped_interior_triangles=int(inward.sum()), method="Split inward shell UVs onto an existing plain ivory cloth patch; no added triangles"),
        lappets=dict(vertices=int(lappets.sum()), skinning='Head to Neck to Spine2 blend; no cloth simulation'),
        validation=fit.validate(target), scope='Fitting candidate; animation and collar intersections reviewed separately')
    (ROOT / 'doc/assets/modular-priest-tripo-helmet-fitting-v2.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
