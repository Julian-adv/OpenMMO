"""Reduce the canonical calf bulge from a preserved source, without changing its rig."""
import copy
import hashlib
import importlib.util
import json
import shutil
from pathlib import Path

import numpy as np

from lib.glb import read_glb, write_glb
from outfits.body_shape import slim_calves

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'assets/modular_human_male_01/fitted/base.glb'
SOURCE = BASE.parents[1] / 'body_shape_sources/base-before-calf-v1.glb'
SOURCE_HASH = '99016ab4311f8fb4256b7a42bdb0d404e2d991154c6ff18c3acb3da76a43f7f0'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    report_path = ROOT / 'doc/assets/modular-male-calf-shape-v1.json'
    known_hashes = {SOURCE_HASH}
    if report_path.exists():
        known_hashes.add(json.loads(report_path.read_text())['output']['sha256'])
    assert digest(BASE) in known_hashes, 'Body changed since this edit; do not overwrite later work'
    if not SOURCE.exists():
        assert digest(BASE) == SOURCE_HASH, 'Unexpected body; preserve and review its source first'
        SOURCE.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(BASE, SOURCE)
    assert digest(SOURCE) == SOURCE_HASH
    spec = importlib.util.spec_from_file_location('plate_io', ROOT / 'tools/fit-modular-plate.py')
    io = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(io)
    original, raw = read_glb(SOURCE)
    doc, binary = copy.deepcopy(original), bytearray(raw)
    fit = lambda p: slim_calves(p, io.BONES)
    changes = []
    for node in doc['nodes']:
        region = node.get('extras', {}).get('region')
        if region not in ('legs', 'ankles', 'boot_ankles', 'feet'):
            continue
        for primitive in doc['meshes'][node['mesh']]['primitives']:
            attrs = primitive['attributes']
            points = io.accessor(doc, raw, attrs['POSITION'])
            positions = fit(points)
            normals = io.accessor(doc, raw, attrs['NORMAL'])
            changed = np.linalg.norm(positions - points, axis=1) > 1e-8
            if not changed.any():
                continue
            adjusted = io.transformed_normals(points, normals, fit)
            adjusted[~changed] = normals[~changed]
            attrs['POSITION'] = io.add_accessor(doc, binary, positions, 'VEC3')
            attrs['NORMAL'] = io.add_accessor(doc, binary, adjusted, 'VEC3')
            if 'TANGENT' in attrs:
                uv = io.accessor(doc, raw, attrs['TEXCOORD_0'])
                faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
                tangents = io.tangents(positions, adjusted, uv, faces)
                tangents[~changed] = io.accessor(doc, raw, attrs['TANGENT'])[~changed]
                attrs['TANGENT'] = io.add_accessor(doc, binary, tangents, 'VEC4')
            changes.append(dict(region=region, vertices_changed=int(changed.sum()),
                maximum_displacement_m=float(np.linalg.norm(positions - points, axis=1).max())))
    doc.setdefault('extras', {})['calf_shape'] = 'v1; posterior radius reduced up to 22% between Y=0.27 and 0.54m'
    write_glb(BASE, doc, io.compact(doc, binary))
    result, data = read_glb(BASE)
    for old_mesh, new_mesh in zip(original['meshes'], result['meshes']):
        for old, new in zip(old_mesh['primitives'], new_mesh['primitives']):
            assert np.array_equal(io.accessor(original, raw, old['indices']), io.accessor(result, data, new['indices']))
            for attr, index in old['attributes'].items():
                a, b = io.accessor(original, raw, index), io.accessor(result, data, new['attributes'][attr])
                if attr not in ('POSITION', 'NORMAL', 'TANGENT'):
                    assert np.array_equal(a, b), attr
                elif attr == 'POSITION':
                    outside = (a[:, 1] <= .27) | (a[:, 1] >= .54)
                    assert np.array_equal(a[outside], b[outside])
                assert np.isfinite(b).all()
    assert original['nodes'] == result['nodes']
    assert np.array_equal(io.accessor(original, raw, original['skins'][0]['inverseBindMatrices']),
                          io.accessor(result, data, result['skins'][0]['inverseBindMatrices']))
    report = dict(source=dict(path=str(SOURCE.relative_to(ROOT)), sha256=digest(SOURCE)),
        output=dict(path=str(BASE.relative_to(ROOT)), sha256=digest(BASE)), changes=changes,
        preserved=['topology', 'UV', 'textures', 'weights', 'rig', 'face', 'ankle cut centers and axes'],
        source_and_license='Existing modular male body; local geometry edit, no additional generation')
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(changes))


if __name__ == '__main__':
    main()
