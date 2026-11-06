"""Turn a Tripo API image-to-model GLB that faces +X into the +Z-facing, floor-origin, unit-height source the humanoid rig expects."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import numpy as np
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
src, dst = Path(sys.argv[1]), Path(sys.argv[2])
doc, data = read_glb(src); b = bytearray(data)
prim = doc['meshes'][0]['primitives'][0]
turn = lambda v: np.stack([-v[:, 2], v[:, 1], v[:, 0]], axis=1)
pos = turn(accessor(doc, b, prim['attributes']['POSITION']).astype(float))
pos[:, 1] -= pos[:, 1].min()
pos /= pos[:, 1].max()
nrm = turn(accessor(doc, b, prim['attributes']['NORMAL']).astype(float))
prim['attributes']['POSITION'] = add_accessor(doc, b, pos, 'VEC3')
prim['attributes']['NORMAL'] = add_accessor(doc, b, nrm, 'VEC3')
doc['nodes'][0].pop('matrix')
doc['buffers'][0]['byteLength'] = len(b)
write_glb(dst, doc, b)
print('min', pos.min(0).round(3), 'max', pos.max(0).round(3))
