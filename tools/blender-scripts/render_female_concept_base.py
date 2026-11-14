"""Render the female base in its symmetric bind A-pose, optionally on heels, as a concept-art underlay.

blender -b --python tools/blender-scripts/render_female_concept_base.py -- --out front.png --view front --heel 30
"""
import argparse
import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--body', default=str(ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'))
parser.add_argument('--out', required=True)
parser.add_argument('--view', choices=['front', 'side', 'back'], default='front')
parser.add_argument('--mannequin', action='store_true', help='Matte blue body, so a generated copy can be told from the outfit by colour')
parser.add_argument('--heel', type=float, default=30, help='Foot pitch of the shared boot heel_pose, degrees')
args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=args.body)
for obj in [o for o in bpy.data.objects if o.name == 'Icosphere']:
    bpy.data.objects.remove(obj)
rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
body = next(o for o in bpy.data.objects if o.type == 'MESH')
# The GLB's node pose and idle1 action are not the bind pose.
rig.animation_data_clear()
for pb in rig.pose.bones:
    pb.matrix_basis = Matrix()
bpy.context.view_layer.update()


def set_matrix(pb, matrix):
    pb.matrix = matrix
    bpy.context.view_layer.update()


def rotate_about_head(pb, axis, degrees):
    head = pb.matrix.to_translation()
    set_matrix(pb, Matrix.Translation(head) @ Matrix.Rotation(math.radians(degrees), 4, axis) @ Matrix.Translation(-head) @ pb.matrix)


# Same rotations as tools/outfits/female_boot_pose.py heel_pose.
for side in ['Left', 'Right']:
    foot, toe = rig.pose.bones[side + 'Foot'], rig.pose.bones[side + 'ToeBase']
    forward = toe.head - foot.head
    forward.z = 0
    axis = Vector((0, 0, 1)).cross(forward.normalized())
    rotate_about_head(foot, axis, args.heel)
    rotate_about_head(toe, axis, -args.heel)

mirror = Matrix.Diagonal((-1, 1, 1, 1))


def average(a, b):
    matrix = a.to_quaternion().slerp(b.to_quaternion(), .5).to_matrix().to_4x4()
    matrix.translation = (a.to_translation() + b.to_translation()) * .5
    return matrix


left = {pb.name: average(pb.matrix, mirror @ rig.pose.bones['Right' + pb.name[4:]].matrix @ mirror)
        for pb in rig.pose.bones if pb.name.startswith('Left')}
for pb in rig.pose.bones:
    if not pb.name.startswith(('Left', 'Right')):
        m = pb.matrix
        centred = ((m.to_3x3() + (mirror @ m @ mirror).to_3x3()) * .5).to_quaternion().to_matrix().to_4x4()
        centred.translation = (0, *m.to_translation()[1:])
        set_matrix(pb, centred)
for pb in rig.pose.bones:
    if pb.name.startswith('Left'):
        set_matrix(pb, left[pb.name])
for pb in rig.pose.bones:
    if pb.name.startswith('Right'):
        set_matrix(pb, mirror @ rig.pose.bones['Left' + pb.name[5:]].matrix @ mirror)

evaluated = body.evaluated_get(bpy.context.evaluated_depsgraph_get())
rig.location.z -= min((evaluated.matrix_world @ v.co).z for v in evaluated.data.vertices)

scene = bpy.context.scene
scene.render.engine = 'CYCLES'
scene.cycles.samples = 64
scene.cycles.device = 'GPU'
scene.render.resolution_x, scene.render.resolution_y = 1024, 1536
scene.view_settings.view_transform = 'Standard'
world = bpy.data.worlds.new('world')
scene.world = world
world.use_nodes = True
world.node_tree.nodes['Background'].inputs[0].default_value = (.62, .62, .62, 1)
camera = bpy.data.cameras.new('camera')
camera.type = 'ORTHO'
camera.ortho_scale = 1.95
camera_obj = bpy.data.objects.new('camera', camera)
scene.collection.objects.link(camera_obj)
scene.camera = camera_obj
if args.view == 'front':
    camera_obj.location, camera_obj.rotation_euler = (0, -5, .9), (math.radians(90), 0, 0)
elif args.view == 'back':
    camera_obj.location, camera_obj.rotation_euler = (0, 5, .9), (math.radians(90), 0, math.radians(180))
else:
    camera_obj.location, camera_obj.rotation_euler = (5, 0, .9), (math.radians(90), 0, math.radians(90))
if args.mannequin:
    blue = bpy.data.materials.new('mannequin')
    blue.use_nodes = True
    shader = blue.node_tree.nodes['Principled BSDF']
    shader.inputs['Base Color'].default_value = (.13, .29, .5, 1)
    shader.inputs['Roughness'].default_value = 1
    for obj in bpy.data.objects:
        if obj.type == 'MESH':
            obj.data.materials.clear()
            obj.data.materials.append(blue)
back = -1 if args.view == 'back' else 1
for location, energy in [((-2, -4 * back, 3), 300), ((2.5, -3 * back, 2), 150), ((0, 4 * back, 3), 150)]:
    light = bpy.data.lights.new('light', 'AREA')
    light.energy, light.size = energy, 3
    light_obj = bpy.data.objects.new('light', light)
    scene.collection.objects.link(light_obj)
    light_obj.location = location
    light_obj.rotation_euler = Vector((0, 0, 1)).rotation_difference(Vector(location)).to_euler()
scene.render.filepath = args.out
bpy.ops.render.render(write_still=True)
