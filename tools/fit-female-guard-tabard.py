"""Skin the women's guard tabard to the female rig as the fitted hauberk under it is skinned.

It was generated over that hauberk, so it stays where the mannequin put it, apart from its girth.

.venv/bin/python tools/fit-female-guard-tabard.py
"""
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'assets/items/female_guard_outfit'
SOURCE = DIRECTORY / 'tripo_tabard_v1/segmented.glb'
HAUBERK = DIRECTORY / 'top/fitted_v1/top_female_guard.glb'
OUTPUT = DIRECTORY / 'tabard/fitted_v1'
PART = 'tabard_female_guard'
# Heights (m, rest pose) of the belt and of the chest picture's top edge, and the picture's width.
BELT = (1.115, 1.205)
PRINT_TOP, PRINT_WIDTH = 1.42, .18
spec = importlib.util.spec_from_file_location('male', ROOT / 'tools/fit-male-guard.py')
male = importlib.util.module_from_spec(spec)
spec.loader.exec_module(male)
io = male.io


def main():
    source, raw = read_glb(SOURCE)
    lift = json.loads((SOURCE.parent / 'split.json').read_text())['heel_lift_m']
    primitive = source['meshes'][0]['primitives'][0]
    positions = io.accessor(source, raw, primitive['attributes']['POSITION']).astype(float) - [0, lift, 0]
    uv = io.accessor(source, raw, primitive['attributes']['TEXCOORD_0'])
    faces = io.accessor(source, raw, primitive['indices']).reshape(-1, 3).astype(int)
    doc, binary = read_glb(HAUBERK)
    binary = bytearray(binary)
    mail = doc['meshes'][0]['primitives'][0]['attributes']
    nearest = cKDTree(io.accessor(doc, binary, mail['POSITION']).astype(float)).query(positions)[1]
    joints = io.accessor(doc, binary, mail['JOINTS_0'])[nearest].astype(np.uint16)
    weights = io.accessor(doc, binary, mail['WEIGHTS_0'])[nearest].astype(float)
    middle = np.array([0, 0, positions[:, 2].mean()])
    positions = (positions - middle) * [male.TABARD_GIRTH, 1, male.TABARD_GIRTH] + middle
    normals = io.smooth_normals(positions, faces)
    spot, picture = male.picture_uv(positions, normals, faces, male.texels(source, raw, uv[faces].mean(1)),
                                    BELT, PRINT_TOP, PRINT_WIDTH)
    corners = faces.reshape(-1)
    positions, normals, uv, joints, weights = (x[corners] for x in (positions, normals, uv, joints, weights))

    # The hauberk's file is the frame: its rig stays and the tabard's mesh, material and texture replace its own.
    views = []
    for image in source['images']:
        view = source['bufferViews'][image['bufferView']]
        start = view.get('byteOffset', 0)
        binary.extend(b'\0' * (-len(binary) % 4))
        doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': view['byteLength']})
        binary += raw[start:start + view['byteLength']]
        views.append(len(doc['bufferViews']) - 1)
    doc['images'] = [{**image, 'bufferView': view} for image, view in zip(source['images'], views)]
    doc['textures'], doc['materials'] = source['textures'], source['materials']
    doc['samplers'] = source.get('samplers', [])
    attributes = io.add_skin_attributes(doc, binary, positions, normals, joints, weights)
    attributes['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    attributes['TEXCOORD_1'] = io.add_accessor(doc, binary, spot.astype(np.float32), 'VEC2')
    doc['meshes'] = [{'name': PART, 'primitives': [{
        'attributes': attributes, 'material': 0,
        'indices': io.add_accessor(doc, binary, np.arange(len(corners)), 'SCALAR', 5125)}]}]
    for node in doc['nodes']:
        if 'mesh' in node:
            node['name'] = PART
            node['extras'] = {**node.get('extras', {}), 'part_id': PART}
    binary = io.compact(doc, binary)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    write_glb(OUTPUT / f'{PART}.glb', doc, binary)
    report = dict(date='2026-11-18', source=male.tuxedo.record(SOURCE), hauberk=male.tuxedo.record(HAUBERK),
                  girth=male.TABARD_GIRTH, triangles=len(faces), picture=picture,
                  output=male.tuxedo.record(OUTPUT / f'{PART}.glb'))
    (OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=1))


if __name__ == '__main__':
    main()
