"""Sync canonical calf geometry into the existing editable outfit files."""
import hashlib
import json
from pathlib import Path

import bpy

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'


def record(path):
    return dict(path=str(path.relative_to(ROOT)), sha256=hashlib.sha256(path.read_bytes()).hexdigest())


records = []
for name in ['character_parts', 'plate_parts', 'barbarian_parts']:
    path = PARTS / f'fitted/{name}.blend'
    bpy.ops.wm.open_mainfile(filepath=str(path))
    targets = [o for o in bpy.data.objects if o.type == 'MESH' and o.get('region') in ('legs', 'ankles', 'boot_ankles', 'feet')]
    assert len(targets) == 4, (name, targets)
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
    imported = set(bpy.data.objects) - before
    for target in targets:
        source = next(o for o in imported if o.type == 'MESH' and o.get('region') == target.get('region'))
        geometry = source.data.copy()
        geometry.transform(target.matrix_world.inverted() @ source.matrix_world)
        assert len(target.data.vertices) == len(geometry.vertices)
        assert [tuple(p.vertices) for p in target.data.polygons] == [tuple(p.vertices) for p in geometry.polygons]
        for old, new in zip(target.data.vertices, geometry.vertices):
            old.co = new.co
        target.data.update()
        target.data.normals_split_custom_set([n.vector for n in geometry.corner_normals])
        assert max((old.co - new.co).length for old, new in zip(target.data.vertices, geometry.vertices)) < 1e-6
        bpy.data.meshes.remove(geometry)
    for obj in imported:
        bpy.data.objects.remove(obj, do_unlink=True)
    bpy.data.orphans_purge(do_recursive=True)
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(path))
    records.append(dict(output=record(path)))
(ROOT / 'doc/assets/modular-male-calf-editable-v1.json').write_text(json.dumps(records, indent=2) + '\n')
