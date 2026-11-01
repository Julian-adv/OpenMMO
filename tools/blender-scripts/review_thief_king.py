import json
import sys
from pathlib import Path

import bpy
from mathutils import Euler, Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
OUT = Path('/tmp/thief-king-review')
DATA = Path('/tmp/thief-king-validation')
fit = json.loads((ROOT / 'assets/monsters/thief_king/grip_v1/fit.json').read_text())
weapon_offset = fit['offset_m']
weapon_rotation = [v * 3.141592653589793 / 180 for v in fit['rotation_degrees_xyz']]
OUT.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(ROOT / 'assets/monsters/thief_king/rig_v1/thief_king.glb'))
mesh = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
for mod in list(mesh.modifiers):
    if mod.type == 'ARMATURE':
        mesh.modifiers.remove(mod)
existing = set(bpy.context.scene.objects)
weapon_name = sys.argv[sys.argv.index('--weapon') + 1] if '--weapon' in sys.argv else 'dagger'
bpy.ops.import_scene.gltf(filepath=str(ROOT / f'client/public/models/weapons/{weapon_name}.glb'))
dagger = [o for o in bpy.context.scene.objects if o not in existing and o.type == 'MESH']
dagger_matrices = [o.matrix_world.copy() for o in dagger]
scene = bpy.context.scene
bpy.ops.mesh.primitive_plane_add(size=200, location=(0, 0, -.003))
plane = bpy.context.object
material = bpy.data.materials.new('Floor')
material.diffuse_color = (.16, .16, .16, 1)
plane.data.materials.append(material)
scene.render.engine = 'CYCLES'
scene.cycles.samples = 16
scene.world = bpy.data.worlds.new('World')
scene.world.color = (.25, .25, .25)
for location, power, size in [((2, -4, 5), 700, 4), ((-3, -2, 3), 450, 3), ((0, 3, 4), 650, 3)]:
    bpy.ops.object.light_add(type='AREA', location=location)
    light = bpy.context.object
    light.data.energy = power
    light.data.shape = 'DISK'
    light.data.size = size
    light.rotation_euler = (Vector((0, 0, 1)) - light.location).to_track_quat('-Z', 'Y').to_euler()
bpy.ops.object.camera_add(location=(0, -6, 2.3))
camera = bpy.context.object
camera.rotation_euler = (Vector((0, 0, 1)) - camera.location).to_track_quat('-Z', 'Y').to_euler()
camera.data.type = 'ORTHO'
camera.data.ortho_scale = 2.9
scene.camera = camera
scene.render.resolution_x = 650
scene.render.resolution_y = 650
scene.render.resolution_percentage = 100
scene.render.image_settings.file_format = 'PNG'
convert = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
clips = ['idle1', 'walk', 'run', 'slash1', 'combat_idle', 'dying']
if '--clip' in sys.argv:
    clips = [sys.argv[sys.argv.index('--clip') + 1]]
for clip in clips:
    for k in ([0] if '--hand-only' in sys.argv else [0, 3, 6]):
        data = json.loads((DATA / f'{clip}-{k}.json').read_text())
        values = data['vertices']
        inverse = mesh.matrix_world.inverted()
        for i, vertex in enumerate(mesh.data.vertices):
            x, y, z = values[i * 3:i * 3 + 3]
            vertex.co = inverse @ Vector((x, -z, y))
        mesh.data.update()
        m = data['hand_matrix']
        hand = Matrix([[m[col * 4 + row] for col in range(4)] for row in range(4)])
        transform = convert @ hand @ Matrix.Translation(weapon_offset) @ Euler(weapon_rotation, 'XYZ').to_matrix().to_4x4() @ convert.inverted()
        for obj, original in zip(dagger, dagger_matrices):
            obj.matrix_world = transform @ original
        scene.render.filepath = str(OUT / f'{clip}-{k}.png')
        if '--hand-only' not in sys.argv:
            bpy.ops.render.render(write_still=True)
        if clip == 'combat_idle' and k == 0:
            focus = convert @ (hand @ Vector((*weapon_offset, 1)))
            old_location, old_rotation, old_scale = camera.location.copy(), camera.rotation_euler.copy(), camera.data.ortho_scale
            camera.location = Vector((focus.x, focus.y - 1.6, focus.z + .35))
            camera.rotation_euler = (focus.xyz - camera.location).to_track_quat('-Z', 'Y').to_euler()
            camera.data.ortho_scale = .50
            old_resolution = (scene.render.resolution_x, scene.render.resolution_y)
            scene.render.resolution_x = scene.render.resolution_y = 1000
            scene.render.filepath = str(OUT / f'right-hand-{weapon_name}.png')
            bpy.ops.render.render(write_still=True)
            direction = convert.to_3x3() @ hand.to_3x3() @ Vector((-.5, .3, -1.2))
            camera.location = focus.xyz + direction
            camera.rotation_euler = (focus.xyz - camera.location).to_track_quat('-Z', 'Y').to_euler()
            scene.render.filepath = str(OUT / f'right-hand-{weapon_name}-palm.png')
            bpy.ops.render.render(write_still=True)
            scene.render.resolution_x, scene.render.resolution_y = old_resolution
            camera.location, camera.rotation_euler, camera.data.ortho_scale = old_location, old_rotation, old_scale
