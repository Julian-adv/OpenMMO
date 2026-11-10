"""Render a female hair over the bind-pose base for a character-creation thumbnail (512², downscale to 128²).

blender -b --python tools/blender-scripts/render_female_hair_thumbnail.py -- --hair <hair.glb> --out bob.png
A face thumbnail passes the fitted face as --body and its hair fit as --hair-move (metres) and --hair-scale
(factors about the head bone), both as right, up, front.
"""
import argparse
import math
import sys
from pathlib import Path

import bpy

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--body', default=str(ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'))
parser.add_argument('--hair', required=True)
parser.add_argument('--hair-move', type=float, nargs=3, default=[0, 0, 0])
parser.add_argument('--hair-scale', type=float, nargs=3, default=[1, 1, 1])
parser.add_argument('--out', required=True)
args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:])

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=args.body)
body = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=args.hair)
# The game's hair fit pivots on the head bone's bind position (femaleHairPivot).
PIVOT = (0, 1.6218, -0.0144)
for obj in set(bpy.data.objects) - body:
    if obj.type != 'MESH':
        continue
    for axis, source in ((0, 0), (2, 1), (1, 2)):
        sign = -1 if axis == 1 else 1
        obj.scale[axis] *= args.hair_scale[source]
        obj.location[axis] += sign * (PIVOT[source] * (1 - args.hair_scale[source]) + args.hair_move[source])
for action in list(bpy.data.actions):
    bpy.data.actions.remove(action)
for obj in bpy.data.objects:
    if obj.type == 'ARMATURE':
        obj.data.pose_position = 'REST'

scene = bpy.context.scene
scene.render.engine = 'BLENDER_EEVEE'
scene.view_settings.view_transform = 'AgX'
scene.view_settings.exposure = -0.5
scene.render.resolution_x = scene.render.resolution_y = 512
scene.render.film_transparent = True
world = bpy.data.worlds.new('world')
world.use_nodes = True
world.node_tree.nodes['Background'].inputs[0].default_value = (0.6, 0.62, 0.7, 1)
world.node_tree.nodes['Background'].inputs[1].default_value = 0.9
scene.world = world
sun = bpy.data.objects.new('key', bpy.data.lights.new('key', 'SUN'))
sun.data.energy = 3.2
sun.data.color = (1, 0.95, 0.88)
sun.rotation_euler = (math.radians(60), 0, math.radians(-20))
scene.collection.objects.link(sun)
camera = bpy.data.objects.new('camera', bpy.data.cameras.new('camera'))
camera.data.type = 'ORTHO'
camera.data.ortho_scale = 0.36
camera.rotation_euler = (math.pi / 2, 0, 0)
camera.location = (0, -3, 1.645)
scene.collection.objects.link(camera)
scene.camera = camera
scene.render.filepath = args.out
bpy.ops.render.render(write_still=True)
