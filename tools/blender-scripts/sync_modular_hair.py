import importlib.util
import json
import sys
from pathlib import Path

import bpy
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
spec = importlib.util.spec_from_file_location('repair', ROOT / 'tools/repair-modular-hair.py')
repair = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repair)
path = repair.PARTS / 'fitted/character_parts.blend'
bpy.ops.wm.open_mainfile(filepath=str(path))
targets = [o for o in bpy.data.objects if o.type == 'MESH' and o.get('part_id') == 'hair_crop']
assert len(targets) == 1
target = targets[0]
world = np.array(target.matrix_world)
axes = np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0]])
linear = axes @ world[:3, :3]
offset = axes @ world[:3, 3]
inverse = np.linalg.inv(linear)
local = np.array([v.co[:] for v in target.data.vertices])
points = local @ linear.T + offset
source, raw = repair.read_glb(repair.SOURCE)
primitive = source['meshes'][0]['primitives'][0]
original = repair.io.accessor(source, raw, primitive['attributes']['POSITION'])
expected = repair.fit(original)
minimum_distance = lambda p, q: np.sqrt(((p[:, None] - q[None]) ** 2).sum(2).min(1)).max()
if minimum_distance(points, expected) > 1e-6:
    assert minimum_distance(points, original) < 1e-6, 'Editable hair changed; review before syncing'
    normals = np.array([n.vector[:] for n in target.data.corner_normals])
    loop_points = points[[loop.vertex_index for loop in target.data.loops]]
    corrected = repair.io.transformed_normals(loop_points, normals @ inverse, repair.fit)
    corrected = corrected @ linear
    corrected /= np.linalg.norm(corrected, axis=1, keepdims=True)
    result = (repair.fit(points) - offset) @ inverse.T
    for vertex, point in zip(target.data.vertices, result):
        vertex.co = point
    target.data.update()
    target.data.normals_split_custom_set(corrected.tolist())
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(path))
actual = np.array([v.co[:] for v in target.data.vertices]) @ linear.T + offset
error = minimum_distance(actual, expected)
assert error < 1e-6
report = json.loads(repair.REPORT.read_text())
report['editable'] = dict(path=str(path.relative_to(ROOT)), sha256=repair.digest(path),
                          maximum_position_error_m=float(error))
repair.REPORT.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report['editable']))
