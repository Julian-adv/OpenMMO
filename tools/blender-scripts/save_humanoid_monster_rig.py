import sys
from pathlib import Path
import bpy

root = Path(__file__).resolve().parents[2]
monster_id = sys.argv[sys.argv.index('--') + 1]
assert monster_id.replace('_', '').isalnum()
path = root / f'assets/monsters/{monster_id}/rig_v1/{monster_id}'
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.context.scene.unit_settings.system = 'METRIC'
bpy.context.scene.unit_settings.scale_length = 1
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.import_scene.gltf(filepath=str(path.with_suffix('.glb')))
for obj in bpy.context.scene.objects:
    if obj.type in {'MESH', 'ARMATURE'}:
        assert all(abs(s - 1) < 1e-6 for s in obj.scale), (obj.name, obj.scale[:])
bpy.ops.wm.save_as_mainfile(filepath=str(path.with_suffix('.blend')))
