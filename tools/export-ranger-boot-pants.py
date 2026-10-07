"""Save the runtime trouser geometry for an identical editable Blender review."""
import importlib.util
import json
import sys
from pathlib import Path

import numpy as np

from lib.glb import view_bytes

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('boots', ROOT / 'tools/fit-tripo-ranger-boots.py')
boots = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boots)
fit, io = boots.fit, boots.io
style = sys.argv[1] if len(sys.argv) > 1 else 'ranger'
assert style in ('ranger', 'rogue')
source = boots.OUTPUT.parent / f'{style}_tripo_pants_v1/pants_{style}.glb'
doc, raw = fit.read_glb(source)
data = json.load(sys.stdin)
assert len(doc['meshes']) == len(data) == 1
images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
binary = bytearray(raw)
for mesh, geometry in zip(doc['meshes'], data):
    primitive = mesh['primitives'][0]
    for name, (attribute, size, kind) in {
        'POSITION': ('position', 3, 'VEC3'), 'NORMAL': ('normal', 3, 'VEC3'),
        'TEXCOORD_0': ('uv', 2, 'VEC2'), 'JOINTS_0': ('skinIndex', 4, 'VEC4'),
        'WEIGHTS_0': ('skinWeight', 4, 'VEC4'),
    }.items():
        values = np.array(geometry[attribute]).reshape(-1, size)
        primitive['attributes'][name] = io.add_accessor(doc, binary, values, kind, 5123 if name == 'JOINTS_0' else 5126)
    primitive['indices'] = io.add_accessor(doc, binary, np.array(geometry['indices']).reshape(-1, 1), 'SCALAR', 5125)
    primitive['attributes'].pop('TANGENT', None)
binary = io.compact(doc, binary)
target = boots.OUTPUT / ('pants_rogue_ranger_boots-review.glb' if style == 'rogue' else 'pants_ranger_boots-review.glb')
fit.write_glb(target, doc, binary)
assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
print(json.dumps(fit.validate(target)))
