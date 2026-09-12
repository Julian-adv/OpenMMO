from pathlib import Path
import argparse
import json
import math
import statistics
import sys

import bpy
from mathutils import Matrix, Quaternion, Vector

ROOT = Path(__file__).resolve().parents[2]
FPS = 24
HEIGHT = 1.8
DEATH_SINK = .10
DEATH_SINK_FRAMES = (36, 48)
DEATH_LEFT_LEG_FRAMES = (48, 60)
MAPPING = {
    'Hip': 'Hips', 'Pelvis': 'Hips', 'Waist': 'Spine',
    'Spine01': 'Spine1', 'Spine02': 'Spine2',
    'NeckTwist01': 'Neck', 'Head': 'Head',
}
for side, prefix in [('L', 'Left'), ('R', 'Right')]:
    for target, source in [('Clavicle', 'Shoulder'), ('Upperarm', 'Arm'),
                           ('Forearm', 'ForeArm'), ('Hand', 'Hand'),
                           ('Thigh', 'UpLeg'), ('Calf', 'Leg'),
                           ('Foot', 'Foot'), ('ToeBase', 'ToeBase')]:
        MAPPING[f'{side}_{target}'] = prefix + source


def snapshot(arm):
    return {b.name: (b.location.copy(), b.rotation_quaternion.copy(), b.scale.copy())
            for b in arm.pose.bones}


def restore(arm, pose, update=True):
    for bone in arm.pose.bones:
        bone.rotation_mode = 'QUATERNION'
        bone.location, bone.rotation_quaternion, bone.scale = pose[bone.name]
    if update:
        bpy.context.view_layer.update()


def shift_root(arm, offset):
    bone = arm.pose.bones['Root']
    matrix = bone.matrix.copy()
    matrix.translation += Vector(offset)
    bone.matrix = matrix
    bpy.context.view_layer.update()


def bounds(mesh):
    evaluated = mesh.evaluated_get(bpy.context.evaluated_depsgraph_get())
    points = [evaluated.matrix_world @ v.co for v in evaluated.data.vertices]
    return [(min(p[i] for p in points), max(p[i] for p in points)) for i in range(3)]


def settle_left_leg(arm, mesh, frames):
    groups = {mesh.vertex_groups[name].index for name in ['L_Foot', 'L_ToeBase']}
    vertices = [v.index for v in mesh.data.vertices
                if any(g.group in groups and g.weight > .5 for g in v.groups)]
    start, end = DEATH_LEFT_LEG_FRAMES
    thigh = arm.pose.bones['L_Thigh']
    for frame in range(start + 1, len(frames)):
        restore(arm, frames[frame])
        original = thigh.matrix.copy()
        location, _, scale = frames[frame]['L_Thigh']
        direction = arm.pose.bones['L_Foot'].head - thigh.head
        axis = Vector((-direction.y, direction.x, 0)).normalized()
        pivot = Matrix.Translation(original.translation)

        def rotate(angle):
            thigh.matrix = pivot @ Quaternion(axis, angle).to_matrix().to_4x4() @ pivot.inverted() @ original
            thigh.location, thigh.scale = location, scale
            thigh.rotation_quaternion.make_compatible(frames[frame]['L_Thigh'][1])
            bpy.context.view_layer.update()
            evaluated = mesh.evaluated_get(bpy.context.evaluated_depsgraph_get())
            return min((evaluated.matrix_world @ evaluated.data.vertices[i].co).z for i in vertices)

        if rotate(0) <= .002:
            continue
        low, high = 0, math.radians(45)
        if rotate(high) > .002:
            raise ValueError(f'Death frame {frame}: left foot cannot reach the floor')
        for _ in range(18):
            angle = (low + high) / 2
            if rotate(angle) > .002:
                low = angle
            else:
                high = angle
        progress = min(1, (frame - start) / (end - start))
        rotate((low + high) / 2 * progress ** 2 * (3 - 2 * progress))
        frames[frame] = snapshot(arm)


def load_motion(path, target):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.fbx(filepath=str(path))
    scene = bpy.context.scene
    if abs(scene.render.fps / scene.render.fps_base - FPS) > .001:
        raise ValueError(f'{path.name}: expected {FPS} fps')
    imported = set(bpy.data.objects) - before
    source = next(o for o in imported if o.type == 'ARMATURE')
    action = source.animation_data.action
    start, end = action.frame_range
    count = round(end - start)
    rest = {name: source.matrix_world @ source.data.bones['mixamorig:' + name].matrix_local
            for name in set(MAPPING.values())}
    factor = target.data.bones['Hip'].head_local.z / rest['Hips'].translation.z
    frames = []
    for i in range(count + 1):
        f = start + (end - start) * i / count
        bpy.context.scene.frame_set(int(f), subframe=f % 1)
        frames.append({name: source.matrix_world @ source.pose.bones['mixamorig:' + name].matrix
                       for name in rest})
    for obj in imported:
        bpy.data.objects.remove(obj, do_unlink=True)
    return rest, frames, factor


def retarget(arm, mesh, rest_pose, source_rest, source_frames, factor, name):
    first = source_frames[0]['Hips'].translation
    travel = source_frames[-1]['Hips'].translation - first
    result, lows, measurements = [], [], []
    for i, source in enumerate(source_frames):
        restore(arm, rest_pose)
        hip = arm.pose.bones['Hip']
        motion = (source['Hips'].translation - source_rest['Hips'].translation) * factor
        if name in ('Walk', 'Run'):
            drift = travel * (i / (len(source_frames) - 1))
            motion.x -= drift.x * factor
            motion.y -= drift.y * factor
        motion.x -= (first.x - source_rest['Hips'].translation.x) * factor
        motion.y -= (first.y - source_rest['Hips'].translation.y) * factor
        for bone in arm.pose.bones:
            source_name = MAPPING.get(bone.name)
            if not source_name:
                continue
            rotation = (source[source_name].to_quaternion()
                        @ source_rest[source_name].to_quaternion().inverted()
                        @ bone.bone.matrix_local.to_quaternion())
            location = bone.head.copy()
            if bone == hip:
                location = bone.bone.head_local + motion
            bone.matrix = Matrix.Translation(location) @ rotation.to_matrix().to_4x4()
            bpy.context.view_layer.update()
        low = bounds(mesh)[2][0]
        lows.append(low)
        measurements.append({n: list(arm.pose.bones[n].head) for n in ['R_Hand', 'L_Hand', 'R_Foot', 'L_Foot']})
        result.append(snapshot(arm))
    constant = min(lows)
    for i, pose in enumerate(result):
        restore(arm, pose)
        ground_shift = .002 - (constant if name == 'Run' else lows[i])
        if name == 'Death':
            start, end = DEATH_SINK_FRAMES
            progress = max(0, min(1, (i - start) / (end - start)))
            ground_shift -= DEATH_SINK * progress ** 2 * (3 - 2 * progress)
        shift_root(arm, (0, 0, ground_shift))
        result[i] = snapshot(arm)
    if name == 'Death':
        settle_left_leg(arm, mesh, result)
    speed = math.hypot(travel.x, travel.y) * factor / ((len(result) - 1) / FPS)
    if name in ('Walk', 'Run') and speed < .1:
        velocities = []
        for foot in ['R_Foot', 'L_Foot']:
            floor = min(m[foot][2] for m in measurements)
            for a, b in zip(measurements, measurements[1:]):
                velocity = (b[foot][1] - a[foot][1]) * FPS
                if a[foot][2] < floor + .10 and b[foot][2] < floor + .10 and velocity > .1:
                    velocities.append(velocity)
        if not velocities:
            raise ValueError(f'Cannot measure {name} ground speed')
        speed = statistics.median(velocities)
    return result, {'duration': (len(result) - 1) / FPS, 'speed': speed, 'samples': measurements}


def render_previews(arm, actions, directory, impact):
    scene = bpy.context.scene
    for track in arm.animation_data.nla_tracks:
        track.mute = True
    camera_data = bpy.data.cameras.new('PreviewCamera')
    camera_data.type = 'ORTHO'
    camera_data.ortho_scale = 2.6
    camera = bpy.data.objects.new('PreviewCamera', camera_data)
    scene.collection.objects.link(camera)
    camera.location = (2.4, -4.4, 2.2)
    camera.rotation_euler = (Vector((0, 0, .9)) - camera.location).to_track_quat('-Z', 'Y').to_euler()
    scene.camera = camera
    scene.world = bpy.data.worlds.new('PreviewWorld')
    scene.world.color = (.35, .35, .35)
    for location, energy in [((2, -3, 4), 500), ((-3, -1, 2), 350), ((0, 3, 3), 450)]:
        data = bpy.data.lights.new('PreviewLight', 'AREA')
        data.energy, data.size = energy, 3
        light = bpy.data.objects.new('PreviewLight', data)
        scene.collection.objects.link(light)
        light.location = location
        light.rotation_euler = (Vector((0, 0, .9)) - light.location).to_track_quat('-Z', 'Y').to_euler()
    bpy.ops.mesh.primitive_plane_add(size=200)
    bpy.context.object.location.z = -.005
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 20
    scene.cycles.use_denoising = True
    scene.render.resolution_x = scene.render.resolution_y = 640
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = 'PNG'
    for name in actions:
        action = actions[name]
        arm.animation_data.action = action
        arm.animation_data.action_slot = action.slots[0]
        times = [impact - .25, impact, impact + .25] if name == 'Attack' else [action.frame_range[1] / FPS if name == 'Death' else .2]
        for t in times:
            scene.frame_set(int(t * FPS), subframe=(t * FPS) % 1)
            scene.render.filepath = str(directory / f'{name}-{round(t * 1000):04d}.png')
            bpy.ops.render.render(write_still=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--preview', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    directory = ROOT / 'assets/skeleton'
    audit = ROOT / '.codex/skeleton-fbx'
    audit.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = FPS
    bpy.ops.import_scene.gltf(filepath=str(directory / 'source.glb'))
    arm = next(o for o in scene.objects if o.type == 'ARMATURE')
    mesh = next(o for o in scene.objects if o.type == 'MESH' and any(m.type == 'ARMATURE' for m in o.modifiers))
    for obj in list(bpy.data.objects):
        if obj not in (arm, mesh):
            bpy.data.objects.remove(obj, do_unlink=True)
    arm.animation_data_clear()
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    for bone in arm.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
        bone.rotation_mode = 'QUATERNION'
    scale = HEIGHT / (max(v.co.z for v in mesh.data.vertices) - min(v.co.z for v in mesh.data.vertices))
    arm.data.transform(Matrix.Scale(scale, 4))
    mesh.data.transform(Matrix.Scale(scale, 4))
    bpy.context.view_layer.update()
    arm.name, mesh.name = 'SkeletonRig', 'Skeleton'
    rest_pose = snapshot(arm)
    for material in mesh.data.materials:
        bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        bsdf.inputs['Metallic'].default_value = 0
        bsdf.inputs['Roughness'].default_value = .85
    baked, report = {}, {}
    for name in ['Idle', 'Walk', 'Run', 'Attack', 'Death']:
        rest, frames, factor = load_motion(directory / 'fbx' / f'{name}.fbx', arm)
        baked[name], report[name] = retarget(arm, mesh, rest_pose, rest, frames, factor, name)
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    actions = {}
    for name, frames in baked.items():
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        arm.animation_data_create()
        arm.animation_data.action = action
        for i, pose in enumerate(frames):
            restore(arm, pose, update=False)
            for bone in arm.pose.bones:
                for prop in ('location', 'rotation_quaternion', 'scale'):
                    bone.keyframe_insert(prop, frame=i, group=bone.name)
        track = arm.animation_data.nla_tracks.new()
        track.name = name
        strip = track.strips.new(name, 0, action)
        strip.action_slot = action.slots[0]
        strip.name = name
        track.mute = True
        actions[name] = action
    arm.animation_data.action = None
    for track in arm.animation_data.nla_tracks:
        track.mute = False
    bpy.ops.object.select_all(action='DESELECT')
    arm.select_set(True)
    mesh.select_set(True)
    bpy.context.view_layer.objects.active = arm
    scene.frame_set(0)
    scene.frame_start, scene.frame_end = 0, len(baked['Attack']) - 1
    output = ROOT / 'client/public/models/monsters/skeleton.glb'
    bpy.ops.wm.save_as_mainfile(filepath=str(directory / 'skeleton.blend'))
    bpy.ops.export_scene.gltf(filepath=str(output), export_format='GLB', use_selection=True,
                              export_animations=True, export_animation_mode='NLA_TRACKS',
                              export_frame_range=False)
    impact = min(range(len(report['Attack']['samples'])), key=lambda i: report['Attack']['samples'][i]['R_Hand'][1]) / FPS
    report['impactSeconds'] = impact
    (audit / 'build-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print('FBX_BUILD', {k: {p:v for p,v in d.items() if p != 'samples'} if isinstance(d,dict) else d for k,d in report.items()})
    if args.preview:
        render_previews(arm, actions, audit, impact)


if __name__ == '__main__':
    main()
