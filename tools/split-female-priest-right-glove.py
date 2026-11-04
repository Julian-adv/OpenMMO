"""Split the right glove, an eye and a shoulder scrap out of the Tripo node `tripo_part_25` by triangle height.

python tools/split-female-priest-right-glove.py <original.glb> <segmented.glb>
"""
import hashlib
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor

src, dst = (Path(a) for a in sys.argv[1:3])
g, raw = read_glb(src)
bin_ = bytearray(raw)

ni = next(i for i, n in enumerate(g['nodes']) if n.get('name') == 'tripo_part_25')
node = g['nodes'][ni]
assert set(node) <= {'name', 'mesh', 'translation'}
ty = node.get('translation', [0, 0, 0])[1]
mesh = g['meshes'][node['mesh']]
assert len(mesh['primitives']) == 1
prim = mesh['primitives'][0]
pos = accessor(g, bin_, prim['attributes']['POSITION'])
idx = accessor(g, bin_, prim['indices']).reshape(-1, 3)
heights = pos[idx, 1].mean(1) + ty
groups = {'right_glove': idx[heights <= .66],
          'tripo_part_25_eye': idx[heights > .85],
          'tripo_part_25_top_fragment': idx[(heights > .66) & (heights <= .85)]}

parent = next(n for n in g['nodes'] if ni in n.get('children', []))
for name, tris in groups.items():
    p = dict(prim)
    p['indices'] = add_accessor(g, bin_, tris.reshape(-1, 1), 'SCALAR', 5125)
    g['bufferViews'][g['accessors'][p['indices']]['bufferView']]['target'] = 34963
    if name == 'right_glove':
        mesh['primitives'] = [p]
        node['name'] = name
    else:
        g['meshes'].append({'name': name, 'primitives': [p]})
        n = dict(node)
        n['name'] = name
        n['mesh'] = len(g['meshes']) - 1
        g['nodes'].append(n)
        parent['children'].append(len(g['nodes']) - 1)
    print(name, len(tris), 'triangles')

g['buffers'][0]['byteLength'] = len(bin_) + (-len(bin_) % 4)
write_glb(dst, g, bytes(bin_))
print('sha256', hashlib.sha256(dst.read_bytes()).hexdigest())
