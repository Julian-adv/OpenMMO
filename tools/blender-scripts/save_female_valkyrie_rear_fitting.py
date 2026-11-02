from pathlib import Path
import sys

import bpy

ROOT = Path(__file__).resolve().parents[2]
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
TOP = ROOT / 'assets/items/valkyrie_starter/tripo_top_v1/fitted_v11/top_female_valkyrie.glb'
version = sys.argv[sys.argv.index('--') + 1] if '--' in sys.argv else 'v7'
BOTTOM = ROOT / f'assets/items/valkyrie_starter/tripo_bottom_v1/fitted_{version}/bottom_female_valkyrie.glb'
OUTPUT = BOTTOM.parent / 'fitting.blend'

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.context.preferences.filepaths.save_version = 0
bpy.context.scene.unit_settings.system = 'METRIC'
bpy.context.scene.unit_settings.scale_length = 1
for source in (BODY, TOP, BOTTOM):
    bpy.ops.import_scene.gltf(filepath=str(source), bone_heuristic='TEMPERANCE')
for rig in (obj for obj in bpy.data.objects if obj.type == 'ARMATURE'):
    rig.animation_data_clear()
    for bone in rig.pose.bones:
        bone.matrix_basis.identity()
bpy.context.view_layer.update()
bpy.ops.file.pack_all()
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT))
