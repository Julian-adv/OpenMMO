import json
import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Euler, Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
OUT = Path('/tmp/thief-king-review/thumb_fix')
DATA = Path('/tmp/thief-king-validation')
args = sys.argv[sys.argv.index('--') + 1:]
weapon_name, clip, sample = args[0], args[1], int(args[2])
fit = json.loads((ROOT / 'assets/monsters/thief_king/grip_v1/fit.json').read_text())
snapshot = json.loads((DATA / f'{clip}-{sample}.json').read_text())
OUT.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(ROOT / 'assets/monsters/thief_king/rig_v1/thief_king.glb'))
mesh = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
for mod in list(mesh.modifiers):
    if mod.type == 'ARMATURE':
        mesh.modifiers.remove(mod)
hand = Matrix([[snapshot['hand_matrix'][c * 4 + r] for c in range(4)] for r in range(4)])
convert = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
hand_inverse = hand.inverted()
values = snapshot['vertices']
local = []
for i, vertex in enumerate(mesh.data.vertices):
    x, y, z = values[i * 3:i * 3 + 3]
    vertex.co = mesh.matrix_world.inverted() @ Vector((x, -z, y))
    local.append(hand_inverse @ Vector((x, y, z)))
mesh.data.update()
bm = bmesh.new()
bm.from_mesh(mesh.data)
bm.verts.ensure_lookup_table()
for face in list(bm.faces):
    inside = [(-.16 < local[v.index].x < .18 and -.19 < local[v.index].y < .30 and -.17 < local[v.index].z < .20) for v in face.verts]
    if not all(inside):
        bm.faces.remove(face)
bm.to_mesh(mesh.data)
bm.free()
existing = set(bpy.context.scene.objects)
bpy.ops.import_scene.gltf(filepath=str(ROOT / f'client/public/models/weapons/{weapon_name}.glb'))
weapons = [o for o in bpy.context.scene.objects if o not in existing and o.type == 'MESH']
rotation = Euler([math.radians(x) for x in fit['rotation_degrees_xyz']], 'XYZ').to_matrix().to_4x4()
transform = convert @ hand @ Matrix.Translation(fit['offset_m']) @ rotation @ convert.inverted()
for obj in weapons:
    obj.matrix_world = transform @ obj.matrix_world
scene = bpy.context.scene
scene.render.engine = 'CYCLES'
scene.cycles.samples = 16
scene.world = bpy.data.worlds.new('World')
scene.world.color = (.28, .28, .28)
scene.world.use_nodes = True
scene.world.node_tree.nodes.get('Background').inputs['Color'].default_value = (.45, .45, .45, 1)
scene.world.node_tree.nodes.get('Background').inputs['Strength'].default_value = .7
focus = (convert @ hand @ Matrix.Translation(fit['offset_m']) @ rotation @ Vector((.025, 0, 0, 1))).xyz
for location, power, size in [((2, -4, 5), 700, 3), ((-3, -2, 3), 600, 3), ((0, 3, 4), 600, 3)]:
    bpy.ops.object.light_add(type='AREA', location=location)
    light = bpy.context.object
    light.data.energy = power
    light.data.size = size
    light.rotation_euler = (focus - light.location).to_track_quat('-Z', 'Y').to_euler()
bpy.ops.object.camera_add()
camera = bpy.context.object
camera.data.type = 'ORTHO'
camera.data.ortho_scale = .32
scene.camera = camera
scene.render.resolution_x = scene.render.resolution_y = 800
scene.render.resolution_percentage = 100
bpy.ops.object.light_add(type='AREA')
fill = bpy.context.object
fill.data.energy = 150
fill.data.size = 1.2
views = {'plus_y': (.18, 1.5, 0), 'plus_z': (.18, 0, 1.5),
         'minus_y': (.18, -1.5, 0), 'minus_z': (.18, 0, -1.5),
         'thumb_side': (.18, -1.1, 1.1)}
if '--view' in args:
    selected = args[args.index('--view') + 1]
    views = {selected: views[selected]}
for view, direction in views.items():
    world_direction = (convert @ hand @ rotation).to_3x3() @ Vector(direction)
    camera.location = focus + world_direction
    fill.location = focus + world_direction.normalized() * 1.1
    fill.rotation_euler = (focus - fill.location).to_track_quat('-Z', 'Y').to_euler()
    camera.rotation_euler = (focus - camera.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.filepath = str(OUT / f'{weapon_name}-{clip}-{view}.png')
    bpy.ops.render.render(write_still=True)
