import json
import math
import sys
from tempfile import TemporaryDirectory
from pathlib import Path

import bpy
import bmesh
import numpy as np
from mathutils import Quaternion, Vector

ROOT = Path(__file__).resolve().parents[2]
args = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
OUTPUT = ROOT / (args[0] if args else 'assets/items/female_knight_boots/reference_v2/fitted_v1')
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import add_accessor

body_doc, body_raw = read_glb(ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb')
body_raw = bytearray(body_raw)
offsets = {entry['name']: entry['lifts'] for entry in json.loads((OUTPUT / 'heel-animation-offsets.json').read_text())}
node_ids = {node.get('name'): i for i, node in enumerate(body_doc['nodes'])}
for animation in body_doc['animations']:
    frames = offsets[animation['name']]
    rotations = frames[0]['rotations']
    replaced = {(node_ids[name], 'rotation') for name in rotations} | {(node_ids['Hips'], 'translation')}
    animation['channels'] = [channel for channel in animation['channels'] if (channel['target']['node'], channel['target']['path']) not in replaced]
    times = add_accessor(body_doc, body_raw, np.array([f['time'] for f in frames])[:, None], 'SCALAR')
    body_doc['accessors'][times].update(min=[frames[0]['time']], max=[frames[-1]['time']])
    for name, path in [(name, 'rotation') for name in rotations] + [('Hips', 'translation')]:
        values = np.array([f['rotations'][name] if path == 'rotation' else np.array(f['hips']) + [0, f['lift'], 0] for f in frames])
        output = add_accessor(body_doc, body_raw, values, 'VEC4' if path == 'rotation' else 'VEC3')
        sampler = len(animation['samplers'])
        animation['samplers'].append({'input': times, 'output': output, 'interpolation': 'LINEAR'})
        animation['channels'].append({'sampler': sampler, 'target': {'node': node_ids[name], 'path': path}})
body_doc['buffers'][0]['byteLength'] = len(body_raw)
bpy.ops.wm.read_factory_settings(use_empty=True)
with TemporaryDirectory() as temp:
    posed_body = Path(temp) / 'body.glb'
    write_glb(posed_body, body_doc, body_raw)
    bpy.ops.import_scene.gltf(filepath=str(posed_body), bone_heuristic='TEMPERANCE')
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
body = bpy.data.objects['female_body']
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'boots_female_knight.glb'), bone_heuristic='TEMPERANCE')
imported = set(bpy.data.objects) - before
for obj in imported:
    if obj.type == 'MESH':
        world = obj.matrix_world.copy()
        obj.parent = rig
        obj.matrix_world = world
        for modifier in obj.modifiers:
            if modifier.type == 'ARMATURE':
                modifier.object = rig
for obj in imported:
    if obj.type != 'MESH':
        bpy.data.objects.remove(obj, do_unlink=True)
if any(obj.vertex_groups.get('LeftKneeGuard') for obj in rig.children if obj.type == 'MESH'):
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')
    for side in ('Left', 'Right'):
        shin = rig.data.edit_bones[side + 'Leg']
        guard = rig.data.edit_bones.new(side + 'KneeGuard')
        guard.parent = shin.parent
        guard.head, guard.tail, guard.roll = shin.head.copy(), shin.tail.copy(), shin.roll
    bpy.ops.object.mode_set(mode='OBJECT')
    for side in ('Left', 'Right'):
        constraint = rig.pose.bones[side + 'KneeGuard'].constraints.new('COPY_ROTATION')
        constraint.target = rig
        constraint.subtarget = side + 'Leg'
        constraint.target_space = constraint.owner_space = 'LOCAL'
        constraint.influence = 0.5
for track in rig.animation_data.nla_tracks:
    track.mute = True
rig.animation_data.action = None
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
for side in ['Left', 'Right']:
    foot, toe = [rig.data.bones[side + suffix] for suffix in ['Foot', 'ToeBase']]
    forward = toe.head_local - foot.head_local
    forward.z = 0
    axis = Vector((0, 0, 1)).cross(forward.normalized())
    for bone, degrees in [(foot, 30), (toe, -30)]:
        local_axis = bone.matrix_local.to_quaternion().inverted() @ axis
        rig.pose.bones[bone.name].rotation_quaternion = Quaternion(local_axis, math.radians(degrees))
lift = json.loads((OUTPUT / 'runtime-validation.json').read_text())['heel_posture']['root_lift']
hips = rig.pose.bones['Hips']
hips.location = rig.data.bones['Hips'].matrix_local.to_quaternion().inverted() @ Vector((0, 0, lift))
bpy.context.view_layer.update()
reference = body.copy()
reference.data = body.data.copy()
bpy.context.collection.objects.link(reference)
reference.name = 'unmasked_body_reference'
reference.hide_render = True
reference.hide_set(True)
mask = json.loads((OUTPUT / 'body-occlusion.json').read_text())
bm = bmesh.new()
bm.from_mesh(body.data)
bm.faces.ensure_lookup_table()
boundaries = [(plane, [bm.faces[i] for i in plane['triangle_indices']]) for plane in mask.get('clip_planes', [])]
bmesh.ops.delete(bm, geom=[bm.faces[i] for i in mask['hide_triangle_indices']], context='FACES')
for plane, faces in boundaries:
    x, y, z = plane['point']
    nx, ny, nz = plane['normal']
    geom = faces + list({edge for face in faces for edge in face.edges}) + list({v for face in faces for v in face.verts})
    bmesh.ops.bisect_plane(bm, geom=geom, plane_co=(x, -z, y), plane_no=(nx, -nz, ny), dist=1e-7, clear_inner=True)
    bmesh.ops.triangulate(bm, faces=[face for face in bm.faces if len(face.verts) > 3])
bm.to_mesh(body.data)
bm.free()
for action in bpy.data.actions:
    action.use_fake_user = True
rig.show_in_front = False
bpy.ops.object.select_all(action='DESELECT')
for obj in rig.children:
    if not obj.name.startswith('boots_female_knight'):
        continue
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type == 'VIEW_3D':
            view = area.spaces.active
            view.overlay.show_overlays = False
            view.shading.type = 'MATERIAL'
            view.region_3d.view_location = (0, 0, .48)
            view.region_3d.view_rotation = Vector((.9, 1.4, .1)).to_track_quat('Z', 'Y')
            view.region_3d.view_distance = 1.65
bpy.ops.file.pack_all()
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'female-knight-boots-fitted.blend'))
if '--pack-only' in args:
    sys.exit(0)
REVIEW = OUTPUT / 'review'
REVIEW.mkdir(exist_ok=True)
scene = bpy.context.scene
scene.render.engine = 'CYCLES'
scene.cycles.samples = 16
scene.cycles.use_denoising = True
scene.world = bpy.data.worlds.new('Review')
scene.world.use_nodes = True
scene.world.node_tree.nodes['Background'].inputs[0].default_value = (.3, .3, .3, 1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value = .8
for location, power in [((3.6, -5.4, 5.4), 972), ((-3.6, -1.8, 3.6), 648), ((0, 5.4, 3.6), 810)]:
    bpy.ops.object.light_add(type='AREA', location=location)
    light = bpy.context.object
    light.data.energy, light.data.size = power, 5.4
    light.rotation_euler = (Vector((0, 0, .72)) - light.location).to_track_quat('-Z', 'Y').to_euler()
bpy.ops.object.camera_add()
camera = bpy.context.object
camera.data.type = 'ORTHO'
scene.camera = camera
scene.render.resolution_x = 900
scene.render.resolution_y = 900
scene.view_settings.view_transform = 'AgX'


def render(name, offset, center=(0, 0, .459), scale=1.152):
    center = Vector(center)
    camera.data.ortho_scale = scale
    camera.location = center + Vector(offset)
    camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.filepath = str(REVIEW / f'{name}.png')
    bpy.ops.render.render(write_still=True)


for name, offset in [('front', (0, -5.4, .18)), ('side', (5.4, 0, .18)), ('back', (0, 5.4, .18)), ('angle', (3.24, -5.4, .54))]:
    render('rest-' + name, offset)
render('full', (2.52, -5.4, .36), (0, 0, .918), 2.052)
for name in ['walk', 'run', 'jump', 'sit_idle']:
    action = next((a for a in bpy.data.actions if a.name == name), None)
    if not action:
        continue
    rig.animation_data.action = action
    rig.animation_data.action_slot = action.slots[0]
    frame = action.frame_range[0] + .35 * (action.frame_range[1] - action.frame_range[0])
    scene.frame_set(int(frame), subframe=frame % 1)
    render('pose-' + name, (3.24, -5.4, .45), (0, 0, .63), 1.53)
