import hashlib
import json
import os
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.mesh_components import welded_islands
from outfits.skinning import deform

OUTPUT = ROOT / 'assets/items/female_caveman_boots/tripo_v1' / f"fitted_{os.environ.get('FEMALE_CAVEMAN_BOOT_FIT_VERSION', 'v4')}"
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
BOOT = OUTPUT / 'boots_female_caveman.glb'


def mesh_data(doc, raw, index):
    p = doc['meshes'][index]['primitives'][0]
    a = p['attributes']
    return (accessor(doc, raw, a['POSITION']), accessor(doc, raw, p['indices']).reshape(-1, 3),
            accessor(doc, raw, a['JOINTS_0']).astype(int), accessor(doc, raw, a['WEIGHTS_0']))


base, body_raw = read_glb(BODY)
bp, bf, bj, bw = mesh_data(base, body_raw, 0)
doc, raw = read_glb(BOOT)
parts = [mesh_data(doc, raw, i) for i in range(2)]
morphs = [accessor(doc, raw, mesh['primitives'][0]['targets'][0]['POSITION']) for mesh in doc['meshes']]
_, _, labels = welded_islands(bp, bf)
skin = np.bincount(labels).argmax()
fitting = json.loads((OUTPUT / 'fitting.json').read_text())
cuff_height = fitting['cuff_height_m']
region = (labels == skin) & (bp[:, 1] > cuff_height-.10) & (bp[:, 1] < cuff_height+.07) & (np.abs(bp[:, 0]) < .30)
mask = json.loads((OUTPUT / 'body-occlusion.json').read_text())
hidden = np.zeros(len(bf), dtype=bool)
hidden[mask['hide_triangle_indices']] = True
samples = json.loads((OUTPUT / 'animation-samples.json').read_text())
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
rig.animation_data_clear()
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
body = bpy.data.objects['female_body']
garments = []
for path in [BOOT]:
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path), bone_heuristic='TEMPERANCE')
    imported = set(bpy.data.objects)-before
    for obj in imported:
        if obj.type == 'MESH':
            world = obj.matrix_world.copy()
            obj.parent = rig
            obj.matrix_world = world
            for modifier in obj.modifiers:
                if modifier.type == 'ARMATURE':
                    modifier.object = rig
            garments.append(obj)
    for obj in imported:
        if obj.type == 'ARMATURE':
            bpy.data.objects.remove(obj, do_unlink=True)
tree = KDTree(len(body.data.polygons))
for face in body.data.polygons:
    center = body.matrix_world @ face.center
    tree.insert(Vector((center.x, center.z, -center.y)), face.index)
tree.balance()
matched = {tree.find(Vector(center))[1] for center in bp[bf[hidden]].mean(1)}
assert len(matched) == int(hidden.sum())
edit = bmesh.new()
edit.from_mesh(body.data)
edit.faces.ensure_lookup_table()
bmesh.ops.delete(edit, geom=[edit.faces[i] for i in matched], context='FACES')
edit.to_mesh(body.data)
edit.free()
conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
canonical = [base['nodes'][index]['name'] for index in base['skins'][0]['joints']]
ordered = sorted(rig.pose.bones, key=lambda bone: len(bone.parent_recursive))
for bone in ordered:
    bone.rotation_mode = 'QUATERNION'
scene = bpy.context.scene
scene.unit_settings.system = 'METRIC'
scene.unit_settings.scale_length = 1
scene.render.engine = 'CYCLES'
scene.cycles.samples = 16
scene.cycles.use_denoising = True
scene.world = bpy.data.worlds.new('Rogue boots studio')
scene.world.use_nodes = True
scene.world.node_tree.nodes['Background'].inputs[0].default_value = (.18, .20, .22, 1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value = .7
for position, power in [((2, -3, 3), 400), ((-3, -1, 2), 240), ((1, 3, 3), 450)]:
    bpy.ops.object.light_add(type='AREA', location=position)
    light = bpy.context.object
    light.data.energy, light.data.size = power, 3
    light.rotation_euler = (Vector((0, 0, .8))-light.location).to_track_quat('-Z', 'Y').to_euler()
bpy.ops.object.camera_add()
camera = bpy.context.object
camera.data.type = 'ORTHO'
scene.camera = camera
scene.render.resolution_x = scene.render.resolution_y = 800
scene.view_settings.view_transform = 'AgX'
(OUTPUT / 'review').mkdir(exist_ok=True)


def set_pose(sample, frame):
    scene.frame_set(frame)
    matrices = dict(zip(canonical, sample['matrices']))
    for bone in ordered:
        transform = np.array(matrices[bone.name]).reshape(4, 4).T.copy()
        transform[1, 3] += sample['lift']
        deformation = Matrix(transform.tolist())
        bone.matrix = rig.matrix_world.inverted() @ conversion @ deformation @ conversion.inverted() @ rig.matrix_world @ bone.bone.matrix_local
        bpy.context.view_layer.update()
        for channel in ['location', 'rotation_quaternion', 'scale']:
            bone.keyframe_insert(data_path=channel, frame=frame)
    for obj in garments:
        if obj.data.shape_keys:
            key = obj.data.shape_keys.key_blocks.get('pose_sit_idle')
            if key:
                key.value = 1 if sample['clip'] == 'sit_idle' else 0
                key.keyframe_insert(data_path='value', frame=frame)


def render(name, offset, target, scale):
    camera.location = Vector(target)+Vector(offset)
    camera.rotation_euler = (Vector(target)-camera.location).to_track_quat('-Z', 'Y').to_euler()
    camera.data.ortho_scale = scale
    scene.render.filepath = str(OUTPUT / 'review' / f'{name}.png')
    bpy.ops.render.render(write_still=True)


initial = next(sample for sample in samples if sample['clip'] == 'idle1' and sample['phase'] == 0)
set_pose(initial, 1)
scene.timeline_markers.new('Equipped standing', frame=1)
for name, offset in [('front', (0, -3, .1)), ('side', (3, 0, .1)), ('back', (0, 3, .1))]:
    render(name, offset, (0, 0, .5), 1.16)
render('sole', (1.5, -2, -1), (0, 0, .12), .65)
for i, name in enumerate(dict.fromkeys(sample['clip'] for sample in samples), 1):
    snapshot = min((sample for sample in samples if sample['clip'] == name), key=lambda sample: abs(sample['phase']-.35))
    set_pose(snapshot, i*10)
    scene.timeline_markers.new(name, frame=i*10)
    if name in ['walk', 'run', 'sit_idle']:
        render(name, (1.6, -3, .6), (0, 0, .55), 1.25)
scene.frame_end = i*10
scene.frame_set(1)
actions = [rig.animation_data.action]+[obj.data.shape_keys.animation_data.action for obj in garments
                                    if obj.data.shape_keys and obj.data.shape_keys.animation_data]
for action in actions:
    for layer in action.layers:
        for strip in layer.strips:
            for slot in action.slots:
                channels = strip.channelbag(slot)
                if channels:
                    for curve in channels.fcurves:
                        for key in curve.keyframe_points:
                            key.interpolation = 'CONSTANT'
camera.location = (1.5, -3, 1.2)
camera.rotation_euler = (Vector((0, 0, .9))-camera.location).to_track_quat('-Z', 'Y').to_euler()
camera.data.ortho_scale = 2.0
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.file.pack_all()
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'female-caveman-boots-fitted.blend'))
