import hashlib
import json
import os
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'assets/modular_human_female_01/hair/tripo_warrior_v1/fitted_v1'
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'), bone_heuristic='TEMPERANCE')
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
if rig.animation_data:
    rig.animation_data.action = None
    for track in rig.animation_data.nla_tracks:
        track.mute = True
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.scene.frame_set(0)
bpy.context.view_layer.update()
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'hair_female_warrior.glb'), bone_heuristic='TEMPERANCE')
imported = set(bpy.data.objects) - before
for obj in imported:
    if obj.type != 'MESH':
        continue
    world = obj.matrix_world.copy()
    obj.parent = rig
    obj.matrix_world = world
    for modifier in obj.modifiers:
        if modifier.type == 'ARMATURE':
            modifier.object = rig
for obj in imported:
    if obj.type != 'MESH':
        bpy.data.objects.remove(obj, do_unlink=True)
for action in bpy.data.actions:
    action.use_fake_user = True
rig.show_in_front = True
bpy.ops.file.pack_all()
bpy.context.preferences.filepaths.save_version = 0
blend = OUTPUT / 'female-warrior-hair-fitted.blend'
bpy.ops.wm.save_as_mainfile(filepath=str(blend))
scene = bpy.context.scene
scene.render.engine = 'BLENDER_WORKBENCH'
scene.display.shading.light = 'STUDIO'
scene.display.shading.color_type = 'TEXTURE'
scene.display.shading.show_shadows = True
scene.display.shading.show_cavity = True
scene.cycles.samples = 16
scene.cycles.use_denoising = True
scene.world = bpy.data.worlds.new('Review')
scene.world.use_nodes = True
scene.world.node_tree.nodes['Background'].inputs[0].default_value = (.18, .18, .18, 1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value = .7
target = Vector((0, 0, 1.296))
for location, energy in [((3.6, -5.4, 5.4), 712.8), ((-3.6, -1.8, 3.6), 486), ((0, 5.4, 3.6), 648)]:
    bpy.ops.object.light_add(type='AREA', location=location)
    light = bpy.context.object
    light.data.energy, light.data.size = energy, 5.4
    light.rotation_euler = (target - light.location).to_track_quat('-Z', 'Y').to_euler()
bpy.ops.object.camera_add()
camera = bpy.context.object
camera.data.type, camera.data.ortho_scale = 'ORTHO', .612
scene.camera = camera
scene.render.resolution_x, scene.render.resolution_y = 650, 850
scene.render.resolution_percentage = 100
scene.view_settings.view_transform = 'AgX'
snapshots = json.loads((OUTPUT / 'animation-snapshots.json').read_text())
for snapshot in snapshots:
    if snapshot['clip'] not in ['walk', 'run', 'jump', 'slash1', 'dying', 'sit_idle']:
        continue
    if os.environ.get('HAIR_REVIEW_CLIP') and snapshot['clip'] != os.environ['HAIR_REVIEW_CLIP']:
        continue
    rig.animation_data.action = bpy.data.actions[snapshot['clip']]
    rig.animation_data.action_slot = rig.animation_data.action.slots[0]
    scene.frame_set(int(snapshot['time'] * scene.render.fps / scene.render.fps_base), subframe=0)
    bpy.context.view_layer.update()
    head = rig.matrix_world @ rig.pose.bones['Head'].head
    center = head + Vector((0, 0, .063))
    for view, offset in [('front', (0, -5.4, .144)), ('back', (0, 5.4, .144))]:
        camera.location = center + Vector(offset)
        camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
        scene.render.filepath = str(OUTPUT / f'motion-{snapshot["clip"]}-{view}.png')
        bpy.ops.render.render(write_still=True)
record_path = ROOT / 'doc/assets/modular-female-warrior-hair-fitting-v1.json'
record = json.loads(record_path.read_text())
record['blender'] = {'path': str(blend.relative_to(ROOT)), 'sha256': hashlib.sha256(blend.read_bytes()).hexdigest(),
                     'actions': [action.name for action in bpy.data.actions], 'bones': len(rig.data.bones)}
record_path.write_text(json.dumps(record, indent=2) + '\n')
