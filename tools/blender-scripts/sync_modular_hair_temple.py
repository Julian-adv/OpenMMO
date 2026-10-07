import importlib.util
import json
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
spec = importlib.util.spec_from_file_location('repair', ROOT / 'tools/repair-modular-hair-temple.py')
repair = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repair)
path = repair.PARTS / 'fitted/character_parts.blend'
bpy.ops.wm.open_mainfile(filepath=str(path))
target = next(o for o in bpy.data.objects if o.type == 'MESH' and o.get('part_id') == 'hair_crop')
doc, raw = repair.read_glb(repair.OUTPUT)
primitive = doc['meshes'][0]['primitives'][0]
attrs = primitive['attributes']
positions = repair.io.accessor(doc, raw, attrs['POSITION'])
uv = repair.io.accessor(doc, raw, attrs['TEXCOORD_0'])
normals = repair.io.accessor(doc, raw, attrs['NORMAL'])
patch = [780, 821, 774]
world = np.array(target.matrix_world)
axes = np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0]])
linear, offset = axes @ world[:3, :3], axes @ world[:3, 3]
before = np.array([v.co[:] for v in target.data.vertices])
points = before @ linear.T + offset
distance = np.linalg.norm(points[:, None] - positions[patch][None], axis=-1)
vertices = distance.argmin(0).tolist()
assert distance.min(0).max() < 1e-6
exists = any(set(p.vertices) == set(vertices) for p in target.data.polygons)
if not exists:
    old_normals = [n.vector[:] for n in target.data.corner_normals]
    old_loops = [loop.vertex_index for loop in target.data.loops]
    mesh = bmesh.new()
    mesh.from_mesh(target.data)
    mesh.verts.ensure_lookup_table()
    face = mesh.faces.new([mesh.verts[i] for i in vertices])
    layer = mesh.loops.layers.uv.active
    assert layer is not None
    for loop, index in zip(face.loops, patch):
        loop[layer].uv = (float(uv[index, 0]), float(1 - uv[index, 1]))
    face.smooth = True
    mesh.to_mesh(target.data)
    mesh.free()
    assert [loop.vertex_index for loop in target.data.loops[:len(old_loops)]] == old_loops
    local_normals = normals[patch] @ linear
    local_normals /= np.linalg.norm(local_normals, axis=1, keepdims=True)
    target.data.normals_split_custom_set(old_normals + local_normals.tolist())
    assert np.array_equal(before, np.array([v.co[:] for v in target.data.vertices]))
    target.data.calc_loop_triangles()
    assert len(target.data.loop_triangles) == 934
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(path))
record_path = ROOT / 'doc/assets/modular-male-hair-temple-cover-v1.json'
report = json.loads(record_path.read_text())
report['editable'] = dict(path=str(path.relative_to(ROOT)), sha256=repair.digest(path), added_faces=1, positions_preserved=True)
record_path.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report['editable']))
