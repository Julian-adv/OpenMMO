import hashlib
import importlib.util
import json
from pathlib import Path

import numpy as np

from lib.glb import read_glb, view_bytes, write_glb

ROOT = Path(__file__).resolve().parents[1]
PARTS = ROOT / 'assets/modular_human_male_01'
SOURCE = PARTS / 'hair/shape_sources/hair_crop-before-temple-cover-v1.glb'
OUTPUT = PARTS / 'fitted/hair_crop.glb'
SOURCE_HASH = '5aea523e5de994b8a2b6e15aef1589a3cbe7413514bc429194e1f0a940141686'
spec = importlib.util.spec_from_file_location('plate_io', ROOT / 'tools/fit-modular-plate.py')
io = importlib.util.module_from_spec(spec)
spec.loader.exec_module(io)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    assert digest(SOURCE) == SOURCE_HASH
    doc, raw = read_glb(SOURCE)
    primitive = doc['meshes'][0]['primitives'][0]
    original_attributes = {name: io.accessor(doc, raw, index).copy() for name, index in primitive['attributes'].items()}
    original_images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    inverse_bind = io.accessor(doc, raw, doc['skins'][0]['inverseBindMatrices']).copy()
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    patch = np.array([[780, 821, 774]], dtype=faces.dtype)
    positions = io.accessor(doc, raw, primitive['attributes']['POSITION'])
    assert np.cross(positions[821] - positions[780], positions[774] - positions[780])[0] > 0
    binary = bytearray(raw)
    primitive['indices'] = io.add_accessor(doc, binary, np.concatenate([faces, patch]).reshape(-1, 1), 'SCALAR', 5125)
    candidate = OUTPUT.with_suffix('.glb.tmp')
    write_glb(candidate, doc, io.compact(doc, binary))
    assert digest(OUTPUT) in {SOURCE_HASH, digest(candidate)}, 'Crop changed since the reviewed source'
    candidate.replace(OUTPUT)
    fitted, fitted_raw = read_glb(OUTPUT)
    target = fitted['meshes'][0]['primitives'][0]
    for name, values in original_attributes.items():
        assert np.array_equal(values, io.accessor(fitted, fitted_raw, target['attributes'][name]))
    assert np.array_equal(faces, io.accessor(fitted, fitted_raw, target['indices']).reshape(-1, 3)[:-1])
    assert original_images == [view_bytes(fitted, fitted_raw, image['bufferView']) for image in fitted['images']]
    assert np.array_equal(inverse_bind, io.accessor(fitted, fitted_raw, fitted['skins'][0]['inverseBindMatrices']))
    assert len(fitted['skins'][0]['joints']) == 65
    report = dict(date='2026-10-08',
        source=dict(path=str(SOURCE.relative_to(ROOT)), sha256=SOURCE_HASH),
        output=dict(path=str(OUTPUT.relative_to(ROOT)), sha256=digest(OUTPUT)),
        source_triangles=len(faces), output_triangles=len(faces) + len(patch),
        added_triangles=patch.tolist(), added_vertices=0,
        method='Bridge the left temple gap using existing vertices from the same UV island; original vertex attributes and triangles unchanged',
        preserved=['positions', 'normals', 'UV', 'skin weights', 'rig', 'materials', 'textures', 'original triangles'],
        source_and_license='Existing modular male crop, local mesh repair; original Meshy Premium and ChatGPT Pro 20x asset records apply. No new AI generation.')
    report_path = ROOT / 'doc/assets/modular-male-hair-temple-cover-v1.json'
    if report_path.exists():
        previous = json.loads(report_path.read_text())
        if previous['output']['sha256'] == report['output']['sha256'] and 'editable' in previous:
            report['editable'] = previous['editable']
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
