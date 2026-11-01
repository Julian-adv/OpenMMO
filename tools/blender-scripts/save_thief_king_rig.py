from pathlib import Path
import bpy

root = Path(__file__).resolve().parents[2]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.context.scene.unit_settings.system = 'METRIC'
bpy.context.scene.unit_settings.scale_length = 1
bpy.ops.import_scene.gltf(filepath=str(root / 'assets/monsters/thief_king/rig_v1/thief_king.glb'))
for obj in bpy.context.scene.objects:
    if obj.type in {'MESH', 'ARMATURE'}:
        assert all(abs(s - 1) < 1e-6 for s in obj.scale), (obj.name, obj.scale[:])
bpy.ops.wm.save_as_mainfile(filepath=str(root / 'assets/monsters/thief_king/rig_v1/thief_king.blend'))
