import hashlib
import importlib.util
import json
from pathlib import Path

import numpy as np

from lib.glb import read_glb, write_glb

ROOT = Path(__file__).resolve().parents[1]
PARTS = ROOT / 'assets/modular_human_male_01'
SOURCE = PARTS / 'hair/shape_sources/hair_crop-before-rear-clearance-v1.glb'
OUTPUT = PARTS / 'fitted/hair_crop.glb'
SOURCE_HASH = '6d754801583d1ae6147ac98cb55fac5237d5b43329e7273d5cb8977c91a0364a'
REPORT = ROOT / 'doc/assets/modular-male-hair-rear-clearance-v1.json'
spec = importlib.util.spec_from_file_location('plate_io', ROOT / 'tools/fit-modular-plate.py')
io = importlib.util.module_from_spec(spec)
spec.loader.exec_module(io)
smoothstep = io.smoothstep


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fit(points):
    result = points.copy()
    influence = smoothstep((points[:, 1] - 1.72) / .035)
    influence *= 1 - smoothstep((points[:, 1] - 1.89) / .035)
    influence *= smoothstep((-points[:, 2] - .025) / .035)
    result[:, 2] -= .004 * influence
    return result


def main():
    assert digest(SOURCE) == SOURCE_HASH
    doc, raw = read_glb(SOURCE)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(doc, raw, attrs['POSITION'])
    normals = io.accessor(doc, raw, attrs['NORMAL'])
    positions = fit(points)
    displacement = np.linalg.norm(positions - points, axis=1)
    changed = displacement > 1e-8
    adjusted = io.transformed_normals(points, normals, fit)
    adjusted[~changed] = normals[~changed]
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    cross = lambda p: np.cross(p[faces[:, 1]] - p[faces[:, 0]], p[faces[:, 2]] - p[faces[:, 0]])
    assert np.all(np.einsum('ij,ij->i', cross(points), cross(positions)) > 0)
    binary = bytearray(raw)
    attrs['POSITION'] = io.add_accessor(doc, binary, positions, 'VEC3')
    attrs['NORMAL'] = io.add_accessor(doc, binary, adjusted, 'VEC3')
    candidate = OUTPUT.with_suffix('.glb.tmp')
    write_glb(candidate, doc, io.compact(doc, binary))
    output_hash = digest(candidate)
    assert digest(OUTPUT) in {SOURCE_HASH, output_hash}, 'Hair changed; review before replacing it'
    candidate.replace(OUTPUT)
    report = json.loads(REPORT.read_text()) if REPORT.exists() else {}
    report.update(source=dict(path=str(SOURCE.relative_to(ROOT)), sha256=SOURCE_HASH),
                  output=dict(path=str(OUTPUT.relative_to(ROOT)), sha256=output_hash),
                  vertices_changed=int(changed.sum()), triangles=len(faces),
                  maximum_displacement_m=float(displacement.max()),
                  preserved=['topology', 'front hairline', 'UV', 'textures', 'skin weights', 'rig'],
                  source_and_license='Existing modular male hair; local geometry edit, no additional generation')
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
