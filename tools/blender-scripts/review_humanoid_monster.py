import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Matrix, Quaternion, Vector

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
sys.path.insert(0, str(ROOT / 'tools/blender-scripts'))
from lib.humanoid_grip import attachment, monster, primary_weapon, weapon_models
from lib.glb import read_glb
from icon_render import add_light

args = sys.argv[sys.argv.index('--') + 1:]
monster_id = args[0]
def flag(name, default=None):
    return args[args.index(name) + 1] if name in args else default

offset, rotation = attachment(monster_id)
row = monster(monster_id)
weapon_name = Path(primary_weapon(monster_id)).stem
source = Path(f'/tmp/{monster_id}-bake/{monster_id}.glb')
doc, _ = read_glb(source)
parents = {child: i for i, n in enumerate(doc['nodes']) for child in n.get('children', [])}
nodes = {n.get('name'): i for i, n in enumerate(doc['nodes'])}
def world(index):
    node = doc['nodes'][index]
    m = Matrix.Translation(node.get('translation', [0, 0, 0])) @ Quaternion(node.get('rotation', [0, 0, 0, 1])[-1:] + node.get('rotation', [0, 0, 0, 1])[:3]).to_matrix().to_4x4()
    return world(parents[index]) @ m if index in parents else m
convert = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
weapon_rotation = (Quaternion((1, 0, 0), rotation[0]) @ Quaternion((0, 1, 0), rotation[1]) @ Quaternion((0, 0, 1), rotation[2])).to_matrix().to_4x4()
weapon_local = convert @ Matrix.Translation(offset) @ weapon_rotation @ convert.inverted()
out = Path(f'/tmp/{monster_id}-review'); out.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(source))
armature = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
mesh = next(o for o in bpy.context.scene.objects if o.type == 'MESH' and o.find_armature())
hand_groups = {g.index for g in mesh.vertex_groups if g.name.startswith('RightHand')}
hand_weights = [sum(g.weight for g in vertex.groups if g.group in hand_groups) for vertex in mesh.data.vertices]
armature.data.pose_position = 'REST'; bpy.context.view_layer.update()
rest = armature.matrix_world @ armature.pose.bones['RightHand'].matrix
correction = rest.inverted() @ convert @ world(nodes['RightHand']) @ convert.inverted()
armature.data.pose_position = 'POSE'
scene = bpy.context.scene
scene.render.engine = 'CYCLES'; scene.cycles.samples = 16
scene.world = bpy.data.worlds.new('World'); scene.world.color = (.25, .25, .25)
add_light((2, -4, 5), 700, 4, (0, 0, 1)); add_light((-3, -2, 3), 450, 3, (0, 0, 1)); add_light((0, 3, 4), 650, 3, (0, 0, 1))
bpy.ops.mesh.primitive_plane_add(size=200, location=(0, 0, -.003))
plane = bpy.context.object
material = bpy.data.materials.new('Floor'); material.diffuse_color = (.16, .16, .16, 1); plane.data.materials.append(material)
bpy.ops.object.camera_add(); camera = bpy.context.object; camera.data.type = 'ORTHO'; scene.camera = camera
scene.render.resolution_x = scene.render.resolution_y = 700
scene.render.resolution_percentage = 100; scene.render.image_settings.file_format = 'PNG'
weapons = {}
for model in weapon_models(monster_id):
    name = Path(model).stem
    before = set(scene.objects)
    bpy.ops.import_scene.gltf(filepath=str(ROOT / 'client/public/models' / model))
    objects = [o for o in scene.objects if o not in before and o.type == 'MESH']
    weapons[name] = [(o, o.matrix_world.copy()) for o in objects]
    for o, _ in weapons[name]: o.hide_render = True

views = {'plus_y': (.18, 1.5, 0), 'plus_z': (.18, 0, 1.5), 'minus_z': (.18, 0, -1.5), 'thumb_side': (.8, -1.2, -.6)}
clips = [flag('--clip')] if flag('--clip') else [action.name for action in bpy.data.actions]
for clip in clips:
    action = bpy.data.actions[clip]; armature.animation_data.action = action
    for index, fraction in enumerate([0, .5, .95]):
        frame = action.frame_range[0] + (action.frame_range[1] - action.frame_range[0]) * fraction
        scene.frame_set(int(frame), subframe=frame % 1)
        bpy.context.view_layer.update()
        hand = armature.matrix_world @ armature.pose.bones['RightHand'].matrix @ correction
        for obj, original in weapons[weapon_name]:
            obj.hide_render = False; obj.matrix_world = hand @ weapon_local @ original
        camera.location = (0, -6, 2.3); target = Vector((0, 0, 1)); camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler(); camera.data.ortho_scale = 2.9
        scene.render.filepath = str(out / f'{clip}-{index}.png')
        bpy.ops.render.render(write_still=True)
        for obj, _ in weapons[weapon_name]: obj.hide_render = True
        if clip not in (row['animAttackIdle'], row['animAttack']) or index != (0 if clip == row['animAttackIdle'] else 1): continue
        depsgraph = bpy.context.evaluated_depsgraph_get()
        evaluated = mesh.evaluated_get(depsgraph)
        frozen = bpy.data.objects.new('closeup', bpy.data.meshes.new_from_object(evaluated)); scene.collection.objects.link(frozen); frozen.matrix_world = mesh.matrix_world.copy()
        bm = bmesh.new(); bm.from_mesh(frozen.data); bm.verts.ensure_lookup_table()
        for face in list(bm.faces):
            if not any(hand_weights[v.index] > .05 for v in face.verts): bm.faces.remove(face)
        bm.to_mesh(frozen.data); bm.free(); mesh.hide_render = True; plane.hide_render = True
        focus = (hand @ weapon_local @ convert @ Vector((.025, 0, 0, 1))).xyz
        for name, items in weapons.items():
            for obj, original in items: obj.hide_render = False; obj.matrix_world = hand @ weapon_local @ original
            for view, direction in views.items():
                axes = (hand @ weapon_local @ convert).to_3x3()
                camera.location = focus + axes @ Vector(direction)
                back = (camera.location - focus).normalized()
                up = axes @ Vector((1, 0, 0)); up = (up - back * up.dot(back)).normalized()
                right = up.cross(back).normalized(); up = back.cross(right)
                camera.rotation_euler = Matrix((right, up, back)).transposed().to_euler(); camera.data.ortho_scale = .32
                scene.render.filepath = str(out / f'{name}-{clip}-{view}.png'); bpy.ops.render.render(write_still=True)
            for obj, _ in items: obj.hide_render = True
        bpy.data.objects.remove(frozen, do_unlink=True); mesh.hide_render = False; plane.hide_render = False
