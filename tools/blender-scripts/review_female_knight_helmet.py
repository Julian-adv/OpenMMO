import json
from pathlib import Path
import bpy
import bmesh
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'assets/items/female_knight_helmet/tripo_v1/fitted_v1'
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(ROOT/'assets/modular_human_female_01/rig_v1/female-base.glb'), bone_heuristic='TEMPERANCE')
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
for track in rig.animation_data.nla_tracks: track.mute = True
rig.animation_data.action = None
for bone in rig.pose.bones: bone.matrix_basis.identity()

def add_part(path):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path), bone_heuristic='TEMPERANCE')
    added = set(bpy.data.objects)-before
    for obj in added:
        if obj.type == 'MESH':
            world = obj.matrix_world.copy(); obj.parent = rig; obj.matrix_world = world
            for mod in obj.modifiers:
                if mod.type == 'ARMATURE': mod.object = rig
        else: bpy.data.objects.remove(obj, do_unlink=True)

add_part(OUT/'helmet_female_knight.glb')
top = ROOT/'assets/items/female_knight_top/tripo_v1/fitted_v4'
add_part(top/'top_female_knight.glb')
body = bpy.data.objects['female_body']
hidden = json.loads((top/'body-occlusion.json').read_text())['hide_triangle_indices']
bm=bmesh.new();bm.from_mesh(body.data);bm.faces.ensure_lookup_table();bmesh.ops.delete(bm,geom=[bm.faces[i] for i in hidden],context='FACES');bm.to_mesh(body.data);bm.free()
for action in bpy.data.actions: action.use_fake_user=True
bpy.ops.file.pack_all();bpy.context.preferences.filepaths.save_version=0
bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'female-knight-helmet-fitted.blend'))
scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.samples=24;scene.cycles.use_denoising=True
scene.world=bpy.data.worlds.new('Studio');scene.world.use_nodes=True
scene.world.node_tree.nodes['Background'].inputs[0].default_value=(.20,.20,.20,1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value=.8
for pos,power in [((3.6,-5.4,5.4),842.4),((-3.6,-1.8,3.6),712.8),((0,5.4,3.6),972)]:
    bpy.ops.object.light_add(type='AREA',location=pos);light=bpy.context.object;light.data.energy=power;light.data.size=5.4;light.rotation_euler=(Vector((0,0,1.71))-light.location).to_track_quat('-Z','Y').to_euler()
bpy.ops.object.camera_add();camera=bpy.context.object;camera.data.type='ORTHO';camera.data.ortho_scale=.504;scene.camera=camera
scene.render.resolution_x=scene.render.resolution_y=900;scene.view_settings.view_transform='AgX'
review=OUT/'review';review.mkdir(exist_ok=True)
for motion in ['rest','combat_idle','run','slash1','sit_idle','dying']:
    if motion!='rest':
        action=next(a for a in bpy.data.actions if a.name==motion);rig.animation_data.action=action;rig.animation_data.action_slot=action.slots[0];scene.frame_set(int(sum(action.frame_range)/2))
    bpy.context.view_layer.update()
    head=rig.pose.bones['Head'];center=rig.matrix_world@head.head+Vector((0,0,.081))
    for view,offset in [('front',(0,-3.6,.216)),('side',(3.6,0,.216)),('back',(0,3.6,.216))]:
        camera.location=center+Vector(offset);camera.rotation_euler=(center-camera.location).to_track_quat('-Z','Y').to_euler();scene.render.filepath=str(review/f'{motion}-{view}.png');bpy.ops.render.render(write_still=True)
