"""Build the horse and rider pose with Blender's background Python runner."""
import math
import sys
import zipfile
from pathlib import Path

import bpy
from mathutils import Matrix, Quaternion, Vector

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/horse'
OUT = ROOT / 'client/public/models'
CLIPS = {'run': (1, 81), 'walk': (82, 163), 'turn_right_180': (164, 278),
         'turn_right_90': (361, 436), 'idle': (437, 937)}
SCALE = 0.01


def select(objects):
    bpy.ops.object.select_all(action='DESELECT')
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]


def export(objects, path, animated=True, materials=True):
    path.parent.mkdir(parents=True, exist_ok=True)
    select(objects)
    bpy.ops.export_scene.gltf(filepath=str(path), export_format='GLB', use_selection=True,
        export_animations=animated, export_animation_mode='ACTIONS', export_frame_range=False,
        export_image_format='WEBP', export_image_quality=90,
        export_materials='EXPORT' if materials else 'NONE')


def keys(arm, frame):
    for bone in arm.pose.bones:
        bone.rotation_mode = 'QUATERNION'
        bone.keyframe_insert('location', frame=frame)
        bone.keyframe_insert('rotation_quaternion', frame=frame)
        bone.keyframe_insert('scale', frame=frame)


def linear(action):
    action.use_fake_user = True
    for layer in action.layers:
        for strip in layer.strips:
            for bag in strip.channelbags:
                for fc in bag.fcurves:
                    for point in fc.keyframe_points:
                        point.interpolation = 'LINEAR'


def horse_turns(arm):
    scene = bpy.context.scene
    select([arm])
    bpy.ops.object.mode_set(mode='EDIT')
    for bone in arm.data.edit_bones:
        bone.use_connect = False
    bpy.ops.object.mode_set(mode='OBJECT')
    rest = {b.name: b.matrix_local.copy() for b in arm.data.bones}
    mirror = Matrix.Diagonal((-1, 1, 1, 1))
    mirrored = {}
    for name, matrix in rest.items():
        opposite = name.replace('Left', 'Right') if name.startswith('Left') else name.replace('Right', 'Left')
        alignment = rest[opposite].inverted() @ mirror @ matrix
        alignment.translation = Vector((0, 0, 0))
        mirrored[name] = opposite, alignment
    for degrees in [90, 180]:
        name = f'turn_right_{degrees}'
        source = bpy.data.actions[name]
        arm.animation_data.action = source
        poses = []
        for frame in range(int(source.frame_range[1]) + 1):
            scene.frame_set(frame)
            hips = arm.pose.bones['Hips'].matrix
            forward = hips.to_quaternion() @ Vector((1, 0, 0))
            yaw = math.atan2(forward.x, -forward.y)
            correction = (Matrix.Rotation(-yaw, 4, 'Z')
                          @ Matrix.Translation((-hips.translation.x, -hips.translation.y, 0)))
            poses.append({b.name: correction @ b.matrix for b in arm.pose.bones})
        arm.animation_data.action = None
        bpy.data.actions.remove(source)
        for side in ['right', 'left']:
            action = bpy.data.actions.new(f'turn_{side}_{degrees}')
            arm.animation_data.action = action
            for frame, pose in enumerate(poses):
                scene.frame_set(frame)
                for bone in arm.pose.bones:
                    if side == 'left':
                        opposite, alignment = mirrored[bone.name]
                        bone.matrix = mirror @ pose[opposite] @ alignment
                    else:
                        bone.matrix = pose[bone.name]
                    bpy.context.view_layer.update()
                keys(arm, frame)
            linear(action)


def horse():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    SOURCE.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(ROOT / 'assets/horse.zip') as archive:
        for name in ['source/full.fbx', 'textures/difuse.png', 'textures/normal.png', 'textures/Horse_PCB.png']:
            path = SOURCE / name
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(archive.read(name))
    bpy.ops.import_scene.fbx(filepath=str(SOURCE / 'source/full.fbx'))
    scene = bpy.context.scene
    scene.render.fps = 30
    arm = bpy.data.objects['Root']
    mesh = bpy.data.objects['Horse']
    poses = {}
    for frame in range(1, 938):
        scene.frame_set(frame)
        poses[frame] = {b.name: arm.matrix_world @ b.matrix for b in arm.pose.bones}
    arm_world = arm.matrix_world.copy()
    mesh_world = mesh.matrix_world.copy()
    for obj in list(scene.objects):
        obj.animation_data_clear()
        if obj not in (arm, mesh):
            bpy.data.objects.remove(obj, do_unlink=True)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    for bone in arm.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    transform = Matrix.Scale(SCALE, 4)
    arm.data.transform(transform @ arm_world)
    arm.matrix_world = Matrix.Identity(4)
    mesh.parent = None
    mesh.matrix_world = Matrix.Identity(4)
    mesh.data.transform(transform @ mesh_world)
    mesh.parent = arm
    mesh.matrix_parent_inverse = Matrix.Identity(4)
    arm.name = 'HorseRig'
    select([arm])
    bpy.ops.object.mode_set(mode='EDIT')
    seat = arm.data.edit_bones.new('RideSeat')
    seat.head = (0, -0.40, 1.63)
    seat.tail = (0, -0.40, 1.73)
    seat.parent = arm.data.edit_bones['Spine2']
    seat.use_deform = False
    bpy.ops.object.mode_set(mode='OBJECT')
    arm.animation_data_create()
    for name, (start, end) in CLIPS.items():
        action = bpy.data.actions.new(name)
        arm.animation_data.action = action
        for frame in range(start, end + 1):
            scene.frame_set(frame - start)
            for bone in arm.pose.bones:
                if bone.name not in poses[frame]:
                    continue
                loc, rot, scale = poses[frame][bone.name].decompose()
                bone.matrix = Matrix.LocRotScale(loc * SCALE, rot, scale)
                bpy.context.view_layer.update()
            keys(arm, frame - start)
        linear(action)
    horse_turns(arm)
    arm.animation_data.action = bpy.data.actions['idle']
    scene.frame_set(0)
    for mat in mesh.data.materials:
        mat.use_nodes = True
        tree = mat.node_tree
        tree.nodes.clear()
        output = tree.nodes.new('ShaderNodeOutputMaterial')
        bsdf = tree.nodes.new('ShaderNodeBsdfPrincipled')
        tree.links.new(bsdf.outputs['BSDF'], output.inputs['Surface'])
        bsdf.inputs['Roughness'].default_value = 0.85
        bsdf.inputs['Specular IOR Level'].default_value = 0.2
        if mat.name == 'HorseHair':
            bsdf.inputs['Base Color'].default_value = (0.035, 0.012, 0.006, 1)
            mat.use_backface_culling = False
            continue
        tex = tree.nodes.new('ShaderNodeTexImage')
        tex.image = bpy.data.images.load(str(SOURCE / 'textures/difuse.png'), check_existing=True)
        tex.image.pack()
        tree.links.new(tex.outputs['Color'], bsdf.inputs['Base Color'])
        normal = tree.nodes.new('ShaderNodeTexImage')
        normal.image = bpy.data.images.load(str(SOURCE / 'textures/normal.png'), check_existing=True)
        normal.image.colorspace_settings.name = 'Non-Color'
        normal.image.scale(1024, 1024)
        normal.image.pack()
        node = tree.nodes.new('ShaderNodeNormalMap')
        tree.links.new(normal.outputs['Color'], node.inputs['Color'])
        tree.links.new(node.outputs['Normal'], bsdf.inputs['Normal'])
    bpy.ops.wm.save_as_mainfile(filepath=str(SOURCE / 'horse.blend'))
    export([arm, mesh], OUT / 'mounts/horse.glb')


def aim(arm, name, direction):
    bone = arm.pose.bones[name]
    current = (bone.tail - bone.head).normalized()
    rot = current.rotation_difference(Vector(direction).normalized()).to_matrix().to_4x4()
    bone.matrix = Matrix.Translation(bone.head) @ rot @ Matrix.Translation(-bone.head) @ bone.matrix
    bpy.context.view_layer.update()


def rider():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(OUT / 'characters/knight.glb'))
    arm = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
    bpy.context.scene.render.fps = 30
    for obj in list(bpy.context.scene.objects):
        if obj.type == 'MESH' and obj.find_armature() != arm:
            bpy.data.objects.remove(obj, do_unlink=True)
    for obj in bpy.context.scene.objects:
        obj.animation_data_clear()
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    arm.animation_data_create()
    arm.animation_data.action = bpy.data.actions.new('ride')
    for bone in arm.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    for side, sign in [('Left', 1), ('Right', -1)]:
        aim(arm, side + 'UpLeg', (sign * 0.32, -0.20, -0.31))
        aim(arm, side + 'Leg', (sign * 0.02, 0.16, -0.46))
        aim(arm, side + 'Foot', (0, -1, -0.08))
        aim(arm, side + 'Arm', (sign * 0.13, -0.20, -0.28))
        aim(arm, side + 'ForeArm', (sign * -0.03, -0.32, 0.02))
        aim(arm, side + 'Hand', (0, -1, -0.15))
    hips = arm.pose.bones['Hips']
    hips.matrix = Matrix.Translation(-hips.head) @ hips.matrix
    bpy.context.view_layer.update()
    base_pose = {bone.name: bone.matrix_basis.copy() for bone in arm.pose.bones}
    for frame in range(121):
        bpy.context.scene.frame_set(frame)
        for bone in arm.pose.bones:
            bone.matrix_basis = base_pose[bone.name]
        bpy.context.view_layer.update()
        phase = math.tau * frame / 120
        breath = math.sin(phase)
        sway = math.sin(phase + math.pi / 4)
        for name, pitch, roll in [
            ('Spine', 0.8, 0.4), ('Spine1', 0.7, 0.25), ('Spine2', 0.5, 0.15),
            ('Neck', -0.6, -0.2), ('Head', -0.4, -0.2),
        ]:
            bone = arm.pose.bones[name]
            rotation = (
                Quaternion((1, 0, 0), math.radians(pitch * breath))
                @ Quaternion((0, 1, 0), math.radians(roll * sway))
            ).to_matrix().to_4x4()
            bone.matrix = Matrix.Translation(bone.head) @ rotation @ Matrix.Translation(-bone.head) @ bone.matrix
            bpy.context.view_layer.update()
        keys(arm, frame)
    linear(arm.animation_data.action)
    bpy.context.scene.frame_set(0)
    bpy.ops.wm.save_as_mainfile(filepath=str(SOURCE / 'rider.blend'))
    export(list(bpy.context.scene.objects), OUT / 'animations/riding.glb', materials=False)


if '--rider-only' not in sys.argv:
    horse()
if '--horse-only' not in sys.argv:
    rider()
