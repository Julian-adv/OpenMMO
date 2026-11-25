"""Render the Floating Eye in matte blue for drawing eyelids over it.

    blender -b -P tools/blender-scripts/render_floating_eye_mannequin.py

Writes front, left and back orthographic views to
assets/monsters/floating_eye/eyelids/mannequin/.
"""
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/monsters/floating_eye/tripo_v1/model_url.glb'
OUT = ROOT / 'assets/monsters/floating_eye/eyelids/mannequin'

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
mesh = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
blue = bpy.data.materials.new('Mannequin')
blue.use_nodes = True
shader = blue.node_tree.nodes['Principled BSDF']
shader.inputs['Base Color'].default_value = (0.02, 0.12, 0.85, 1)
shader.inputs['Roughness'].default_value = 0.9
mesh.data.materials.clear()
mesh.data.materials.append(blue)

scene = bpy.context.scene
scene.render.engine = 'BLENDER_EEVEE'
scene.render.resolution_x = scene.render.resolution_y = 1254
scene.view_settings.view_transform = 'Standard'
scene.world = bpy.data.worlds.new('w')
scene.world.use_nodes = True
background = scene.world.node_tree.nodes['Background']
background.inputs[0].default_value = (0.75, 0.75, 0.75, 1)
sun = bpy.data.objects.new('sun', bpy.data.lights.new('sun', 'SUN'))
sun.data.energy = 2.5
scene.collection.objects.link(sun)
camera = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
scene.collection.objects.link(camera)
scene.camera = camera
camera.data.type = 'ORTHO'
camera.data.ortho_scale = 1.25
# The API model looks along +X: its left is +Y.
for name, direction in [('front', (1, 0, 0)), ('left', (0, 1, 0)), ('back', (-1, 0, 0))]:
    camera.location = Vector(direction) * 5
    camera.rotation_euler = (-camera.location).to_track_quat('-Z', 'Y').to_euler()
    sun.rotation_euler = camera.rotation_euler
    scene.render.filepath = str(OUT / f'{name}.png')
    bpy.ops.render.render(write_still=True)
