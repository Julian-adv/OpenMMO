import argparse
import sys
from pathlib import Path

import bpy

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts/fitted'
parser = argparse.ArgumentParser()
parser.add_argument('--outfit', choices=['plate', 'barbarian'], default='plate')
args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(PARTS / 'base.glb'))
rig = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
for obj in bpy.context.scene.objects:
    if obj.type == 'MESH':
        obj.hide_render = (obj.get('region') != 'head' if args.outfit == 'plate'
                           else obj.get('region') == 'boot_ankles')
        obj.hide_set(obj.hide_render)

for slot in ['top', 'pants', 'boots', 'gloves', 'helmet']:
    name = f'{slot}_{args.outfit}'
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(PARTS / f'{name}.glb'))
    imported = set(bpy.data.objects) - before
    for obj in imported:
        if obj.type != 'MESH' or not obj.get('part_id'):
            continue
        world = obj.matrix_world.copy()
        obj.parent = rig
        obj.matrix_world = world
        for modifier in obj.modifiers:
            if modifier.type == 'ARMATURE':
                modifier.object = rig
    for obj in imported:
        if obj.type == 'ARMATURE':
            bpy.data.objects.remove(obj, do_unlink=True)

for bone in rig.pose.bones:
    bone.custom_shape = None
for obj in list(bpy.data.objects):
    if obj.type == 'MESH' and not obj.get('part_id'):
        bpy.data.objects.remove(obj, do_unlink=True)
bpy.ops.file.pack_all()
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.wm.save_as_mainfile(filepath=str(PARTS / f'{args.outfit}_parts.blend'))
