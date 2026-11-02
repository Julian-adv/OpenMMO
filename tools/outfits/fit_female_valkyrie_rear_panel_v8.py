import hashlib
import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.mesh_components import welded_islands

SOURCE = ROOT / 'assets/items/valkyrie_starter/tripo_bottom_v1/fitted_v7/bottom_female_valkyrie.glb'
OUTPUT = SOURCE.parent.parent / 'fitted_v8'
assert hashlib.sha256(SOURCE.read_bytes()).hexdigest() == '8ea403f7956167d89c8c7bc57ef38d473870cf733bafde0fb42b06a9191fcf4d'
OUTPUT.mkdir(exist_ok=True)
doc, binary = read_glb(SOURCE)
raw = bytearray(binary)
primitive = doc['meshes'][0]['primitives'][0]
attrs = primitive['attributes']
positions = accessor(doc, binary, attrs['POSITION'])
normals = accessor(doc, binary, attrs['NORMAL'])
tangents = accessor(doc, binary, attrs['TANGENT'])
faces = accessor(doc, binary, primitive['indices']).reshape(-1, 3)
assert len(positions) == 4251 and len(faces) == 3477
_, _, labels = welded_islands(positions, faces)
panel_ids = np.arange(3534, 4217)
components = sorted(np.unique(labels[panel_ids]),
                    key=lambda label: np.count_nonzero(labels[panel_ids] == label), reverse=True)
assert len(components) == 6 and np.count_nonzero(labels[panel_ids] == components[0]) == 359


def offset(height):
    t = np.clip((height - .99) / .12, 0, 1)
    return .015 * t * t * (3 - 2 * t)


for component in components:
    ids = panel_ids[labels[panel_ids] == component]
    if component != components[0]:
        positions[ids, 2] += offset(positions[ids, 1].mean())
        continue
    t = np.clip((positions[ids, 1] - .99) / .12, 0, 1)
    derivative = .015 / .12 * 6 * t * (1 - t)
    positions[ids, 2] += offset(positions[ids, 1])
    transformed_normals = normals[ids].copy()
    transformed_normals[:, 1] -= derivative * transformed_normals[:, 2]
    transformed_normals /= np.linalg.norm(transformed_normals, axis=1)[:, None]
    transformed_tangents = tangents[ids, :3].copy()
    transformed_tangents[:, 2] += derivative * transformed_tangents[:, 1]
    transformed_tangents -= transformed_normals * np.sum(transformed_normals * transformed_tangents, axis=1)[:, None]
    transformed_tangents /= np.linalg.norm(transformed_tangents, axis=1)[:, None]
    normals[ids] = transformed_normals
    tangents[ids, :3] = transformed_tangents

for name, data in [('POSITION', positions), ('NORMAL', normals), ('TANGENT', tangents)]:
    source = doc['accessors'][attrs[name]]
    attrs[name] = add_accessor(doc, raw, data, source['type'], source['componentType'])
doc['buffers'] = [{'byteLength': len(raw)}]
output = OUTPUT / 'bottom_female_valkyrie.glb'
write_glb(output, doc, raw)
mask = json.loads((SOURCE.parent / 'body-occlusion.json').read_text())
mask['bottom_sha256'] = hashlib.sha256(output.read_bytes()).hexdigest()
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')
report = {'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
          'bottom_sha256': mask['bottom_sha256'], 'triangles': len(faces),
          'maximum_upper_inward_offset_m': .015,
          'status': 'Rear panel fitting candidate; motion and visual review required.'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
