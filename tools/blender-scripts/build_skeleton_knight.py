from pathlib import Path
import argparse
import json
import math
import statistics
import sys

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
FPS = 24
BAKE_FPS = 72
HEIGHT = 2.05
GRIP = Vector((0, .11, 0))
TIP = Vector((1.48, 0, 0))
SWORD_POINTS = []
MAPPING = {
    "Hip": "Hips",
    "Pelvis": "Hips",
    "Waist": "Spine",
    "Spine01": "Spine1",
    "Spine02": "Spine2",
    "NeckTwist01": "Neck",
    "Head": "Head",
}
for side, prefix in [("L", "Left"), ("R", "Right")]:
    for target, source in [
        ("Clavicle", "Shoulder"),
        ("Upperarm", "Arm"),
        ("Forearm", "ForeArm"),
        ("Hand", "Hand"),
        ("Thigh", "UpLeg"),
        ("Calf", "Leg"),
        ("Foot", "Foot"),
        ("ToeBase", "ToeBase"),
    ]:
        MAPPING[f"{side}_{target}"] = prefix + source


def snapshot(armature):
    return {
        bone.name: (
            bone.location.copy(),
            bone.rotation_quaternion.copy(),
            bone.scale.copy(),
        )
        for bone in armature.pose.bones
    }


def restore(armature, pose, update=True):
    for bone in armature.pose.bones:
        bone.rotation_mode = "QUATERNION"
        bone.location, bone.rotation_quaternion, bone.scale = pose[bone.name]
    if update:
        bpy.context.view_layer.update()


def shift_root(armature, offset):
    bone = armature.pose.bones["Root"]
    matrix = bone.matrix.copy()
    matrix.translation += Vector(offset)
    bone.matrix = matrix
    bpy.context.view_layer.update()


def bounds(mesh):
    evaluated = mesh.evaluated_get(bpy.context.evaluated_depsgraph_get())
    points = [evaluated.matrix_world @ vertex.co for vertex in evaluated.data.vertices]
    return [
        (min(point[axis] for point in points), max(point[axis] for point in points))
        for axis in range(3)
    ]


def load_motion(path, target):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.fbx(filepath=str(path))
    scene = bpy.context.scene
    if abs(scene.render.fps / scene.render.fps_base - FPS) > 0.001:
        raise ValueError(f"{path.name}: expected {FPS} fps")
    imported = set(bpy.data.objects) - before
    source = next(obj for obj in imported if obj.type == "ARMATURE")
    action = source.animation_data.action
    start, end = action.frame_range
    count = round(end - start)
    rest = {
        name: source.matrix_world @ source.data.bones["mixamorig:" + name].matrix_local
        for name in set(MAPPING.values())
    }
    factor = target.data.bones["Hip"].head_local.z / rest["Hips"].translation.z
    frames = []
    for index in range(count + 1):
        frame = start + (end - start) * index / count
        scene.frame_set(int(frame), subframe=frame % 1)
        frames.append(
            {
                name: source.matrix_world @ source.pose.bones["mixamorig:" + name].matrix
                for name in rest
            }
        )
    for obj in imported:
        bpy.data.objects.remove(obj, do_unlink=True)
    return rest, frames, factor


def retarget(armature, mesh, rest_pose, source_rest, source_frames, factor, name):
    first = source_frames[0]["Hips"].translation
    travel = source_frames[-1]["Hips"].translation - first
    result = []
    lows = []
    measurements = []
    for index, source in enumerate(source_frames):
        restore(armature, rest_pose)
        hip = armature.pose.bones["Hip"]
        motion = (source["Hips"].translation - source_rest["Hips"].translation) * factor
        if name in ("Walk", "Run"):
            drift = travel * (index / (len(source_frames) - 1))
            motion.x -= drift.x * factor
            motion.y -= drift.y * factor
        motion.x -= (first.x - source_rest["Hips"].translation.x) * factor
        motion.y -= (first.y - source_rest["Hips"].translation.y) * factor
        for bone in armature.pose.bones:
            source_name = MAPPING.get(bone.name)
            if not source_name:
                continue
            rotation = (
                source[source_name].to_quaternion()
                @ source_rest[source_name].to_quaternion().inverted()
                @ bone.bone.matrix_local.to_quaternion()
            )
            location = bone.head.copy()
            if bone == hip:
                location = bone.bone.head_local + motion
            bone.matrix = Matrix.Translation(location) @ rotation.to_matrix().to_4x4()
            bpy.context.view_layer.update()
        lows.append(bounds(mesh)[2][0])
        measurements.append(
            {
                bone_name: list(armature.pose.bones[bone_name].head)
                for bone_name in ["R_Hand", "L_Hand", "R_Foot", "L_Foot"]
            }
        )
        result.append(snapshot(armature))
    constant = min(lows)
    for index, pose in enumerate(result):
        restore(armature, pose)
        shift_root(
            armature,
            (0, 0, 0.002 - (constant if name == "Run" else lows[index])),
        )
        result[index] = snapshot(armature)
    speed = math.hypot(travel.x, travel.y) * factor / ((len(result) - 1) / FPS)
    if name in ("Walk", "Run") and speed < 0.1:
        velocities = []
        for foot in ["R_Foot", "L_Foot"]:
            floor = min(measurement[foot][2] for measurement in measurements)
            for current, following in zip(measurements, measurements[1:]):
                velocity = (following[foot][1] - current[foot][1]) * FPS
                if (
                    current[foot][2] < floor + 0.10
                    and following[foot][2] < floor + 0.10
                    and velocity > 0.1
                ):
                    velocities.append(velocity)
        if not velocities:
            raise ValueError(f"Cannot measure {name} ground speed")
        speed = statistics.median(velocities)
    return result, {
        "duration": (len(result) - 1) / FPS,
        "speed": speed,
        "samples": measurements,
    }


def render_previews(armature, actions, directory, impact):
    scene = bpy.context.scene
    for track in armature.animation_data.nla_tracks:
        track.mute = True
    camera_data = bpy.data.cameras.new("PreviewCamera")
    camera_data.type = "ORTHO"
    camera_data.ortho_scale = 2.8
    camera = bpy.data.objects.new("PreviewCamera", camera_data)
    scene.collection.objects.link(camera)
    camera.location = (2.5, -4.6, 2.3)
    camera.rotation_euler = (
        Vector((0, 0, 0.95)) - camera.location
    ).to_track_quat("-Z", "Y").to_euler()
    scene.camera = camera
    scene.world = bpy.data.worlds.new("PreviewWorld")
    scene.world.color = (0.35, 0.35, 0.35)
    for location, energy in [((2, -3, 4), 500), ((-3, -1, 2), 350), ((0, 3, 3), 450)]:
        data = bpy.data.lights.new("PreviewLight", "AREA")
        data.energy, data.size = energy, 3
        light = bpy.data.objects.new("PreviewLight", data)
        scene.collection.objects.link(light)
        light.location = location
        light.rotation_euler = (
            Vector((0, 0, 0.95)) - light.location
        ).to_track_quat("-Z", "Y").to_euler()
    bpy.ops.mesh.primitive_plane_add(size=200)
    bpy.context.object.location.z = -0.005
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = scene.render.resolution_y = 640
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    for name, action in actions.items():
        armature.animation_data.action = action
        armature.animation_data.action_slot = action.slots[0]
        times = [impact - 0.25, impact, impact + 0.25] if name == "Attack" else [action.frame_range[1] / BAKE_FPS if name == "Death" else 0.2]
        for moment in times:
            scene.frame_set(int(moment * BAKE_FPS), subframe=(moment * BAKE_FPS) % 1)
            scene.render.filepath = str(directory / f"{name}-{round(moment * 1000):04d}.png")
            bpy.ops.render.render(write_still=True)


def solve_arm(armature, side, wrist, pole):
    upper = armature.pose.bones[side + '_Upperarm']
    fore = armature.pose.bones[side + '_Forearm']
    hand = armature.pose.bones[side + '_Hand']
    shoulder = upper.head.copy()
    a = (fore.head - shoulder).length
    b = (hand.head - fore.head).length
    direction = wrist - shoulder
    distance = min(direction.length, (a+b)*.995)
    direction.normalize()
    wrist = shoulder + direction*distance
    along = (a*a - b*b + distance*distance) / (2*distance)
    outward = pole - shoulder
    outward -= direction*outward.dot(direction)
    outward.normalize()
    elbow = shoulder + direction*along + outward*math.sqrt(max(0, a*a-along*along))
    for bone, child, target in [(upper, fore, elbow), (fore, hand, wrist)]:
        origin = bone.head.copy()
        rotation = (child.head-origin).rotation_difference(target-origin) @ bone.matrix.to_quaternion()
        bone.matrix = Matrix.Translation(origin) @ rotation.to_matrix().to_4x4()
        bpy.context.view_layer.update()


def dragging_pose(armature):
    upper = armature.pose.bones['R_Upperarm']
    fore = armature.pose.bones['R_Forearm']
    hand = armature.pose.bones['R_Hand']
    shoulder = upper.head.copy()
    reach = (fore.head-shoulder).length + (hand.head-fore.head).length
    wrist = shoulder + Vector((-.35, .24, -1)).normalized()*reach*.94
    solve_arm(armature, 'R', wrist, shoulder+Vector((-.5, .15, -.3)))
    wrist = hand.head.copy()
    height = wrist.z
    for _ in range(8):
        down = (.015-height)/TIP.x
        blade = Vector((-.08, math.sqrt(max(.01, 1-down*down-.08**2)), down))
        fingers = Vector((0, 0, -1))
        fingers -= blade*fingers.dot(blade)
        fingers.normalize()
        normal = blade.cross(fingers).normalized()
        rotation = Matrix((blade, fingers, normal)).transposed()
        height = (wrist + rotation @ GRIP).z
    hand.matrix = Matrix.Translation(wrist) @ rotation.to_4x4()
    bpy.context.view_layer.update()


def keep_blade_above_floor(armature, clearance=.008):
    hand = armature.pose.bones['R_Hand']
    wrist = hand.head.copy()
    original = hand.matrix.to_3x3()
    def low(rotation):
        return min((wrist + rotation @ (GRIP+p)).z for p in SWORD_POINTS)
    if low(original) >= clearance:
        return
    blade = original.col[0].copy()
    axis = blade.cross(Vector((0, 0, 1)))
    if axis.length < .001:
        axis = Vector((1, 0, 0))
    axis.normalize()
    chosen = original
    for step in range(1, 91):
        chosen = Matrix.Rotation(math.radians(step), 3, axis) @ original
        if low(chosen) >= clearance:
            lower, upper = step-1, step
            for _ in range(10):
                mid = (lower+upper)/2
                trial = Matrix.Rotation(math.radians(mid), 3, axis) @ original
                if low(trial) >= clearance:
                    upper = mid
                else:
                    lower = mid
            chosen = Matrix.Rotation(math.radians(upper), 3, axis) @ original
            break
    else:
        # A fallen hand can lie below the guard; turn the blade flat beside it.
        blade = Vector((-.8, .6, 0))
        chosen = Matrix((blade, Vector((0, 0, 1)).cross(blade), Vector((0, 0, 1)))).transposed()
        wrist.z += max(0, clearance-low(chosen))
        shoulder = armature.pose.bones['R_Upperarm'].head.copy()
        solve_arm(armature, 'R', wrist, shoulder+Vector((-.5, 0, .2)))
        wrist = hand.head.copy()
    wrist.z += max(0, clearance-low(chosen))
    hand.matrix = Matrix.Translation(wrist) @ chosen.to_4x4()
    bpy.context.view_layer.update()


def left_grip(armature):
    right = armature.pose.bones['R_Hand']
    left = armature.pose.bones['L_Hand']
    rotation = right.matrix.to_3x3()
    grip = right.matrix @ GRIP
    left_rotation = rotation @ Matrix.Rotation(math.pi, 3, 'X')
    target = grip - rotation @ Vector((.20, 0, 0)) - left_rotation @ GRIP
    shoulder = armature.pose.bones['L_Upperarm'].head.copy()
    fore = armature.pose.bones['L_Forearm']
    reach = (fore.head-shoulder).length + (left.head-fore.head).length
    for _ in range(4):
        if (target-shoulder).length <= reach*.98:
            break
        correction = shoulder + (target-shoulder).normalized()*reach*.98 - target
        wrist = right.head + correction
        right_shoulder = armature.pose.bones['R_Upperarm'].head.copy()
        solve_arm(armature, 'R', wrist, right_shoulder+Vector((-.5, -.2, -.3)))
        right.matrix = Matrix.Translation(right.head) @ rotation.to_4x4()
        bpy.context.view_layer.update()
        grip = right.matrix @ GRIP
        target = grip - rotation @ Vector((.20, 0, 0)) - left_rotation @ GRIP
    solve_arm(armature, 'L', target, shoulder+Vector((.5, -.2, -.3)))
    left.matrix = Matrix.Translation(left.head) @ left_rotation.to_4x4()
    bpy.context.view_layer.update()


def align_two_handed_slash(armature, source_direction):
    right = armature.pose.bones['R_Hand']
    left = armature.pose.bones['L_Hand']
    blade = source_direction.normalized()
    if blade.z < -.25:
        horizontal = Vector((blade.x, blade.y, 0)).normalized()
        blade = horizontal*math.sqrt(1-.25**2) + Vector((0, 0, -.25))
    fingers = right.matrix.to_3x3().col[1].copy()
    fingers -= blade*fingers.dot(blade)
    fingers.normalize()
    normal = blade.cross(fingers).normalized()
    rotation = Matrix((blade, fingers, normal)).transposed()
    right.matrix = Matrix.Translation(right.head) @ rotation.to_4x4()
    bpy.context.view_layer.update()


def blend_pose(first, last, factor):
    return {name: (first[name][0].lerp(last[name][0], factor),
                   first[name][1].slerp(last[name][1], factor),
                   first[name][2].lerp(last[name][2], factor)) for name in first}


def author_weapon_motion(armature, mesh, baked, source_directions, death_directions):
    for name in ['Idle', 'Walk', 'Run']:
        for index, pose in enumerate(baked[name]):
            restore(armature, pose)
            dragging_pose(armature)
            keep_blade_above_floor(armature)
            baked[name][index] = snapshot(armature)
    slash = []
    for frame, pose in enumerate(baked['Attack']):
        restore(armature, pose)
        align_two_handed_slash(armature, source_directions[frame])
        keep_blade_above_floor(armature, .035)
        left_grip(armature)
        slash.append(snapshot(armature))
    ready = baked['Idle'][0]
    lead = 10
    recover = 14
    baked['Attack'] = [blend_pose(ready, slash[0], (i/lead)**2*(3-2*i/lead)) for i in range(lead)] + slash
    baked['Attack'] += [blend_pose(slash[-1], ready, (i/recover)**2*(3-2*i/recover)) for i in range(1, recover+1)]
    samples = []
    for index, pose in enumerate(baked['Attack']):
        restore(armature, pose)
        low = bounds(mesh)[2][0]
        shift_root(armature, (0, 0, .002-low))
        keep_blade_above_floor(armature)
        if lead <= index < lead+len(slash):
            left_grip(armature)
        baked['Attack'][index] = snapshot(armature)
        hand = armature.pose.bones['R_Hand']
        tip = hand.matrix @ (GRIP+TIP)
        samples.append(list(tip))
    for index, pose in enumerate(baked['Death']):
        restore(armature, pose)
        progress = max(0, min(1, (index/FPS-1.8)/.7))
        shift_root(armature, (0, 0, -.15*progress*progress*(3-2*progress)))
        baked['Death'][index] = snapshot(armature)
    # Evaluate the blade in the supplied slash, excluding the two transitions.
    impact = min(range(lead, lead+len(slash)), key=lambda i: samples[i][1])
    return {'duration': (len(baked['Attack'])-1)/FPS,
            'impactSeconds': impact/FPS, 'bladeTipSamples': samples,
            'windupSeconds': lead/FPS, 'recoverySeconds': recover/FPS}


def resample_weapon_motion(armature, baked):
    for name, frames in baked.items():
        expanded = []
        for index in range((len(frames)-1)*3+1):
            lower = min(index//3, len(frames)-1)
            upper = min(lower+1, len(frames)-1)
            pose = blend_pose(frames[lower], frames[upper], (index%3)/3)
            restore(armature, pose)
            if name == 'Attack':
                if 30 <= index <= 120:
                    align_two_handed_slash(armature, armature.pose.bones['R_Hand'].matrix.to_3x3().col[0].copy())
                    for _ in range(12):
                        left_grip(armature)
                        keep_blade_above_floor(armature, .025)
                else:
                    keep_blade_above_floor(armature, .025 if index not in (0, len(frames)*3-3) else .008)
            expanded.append(snapshot(armature))
        baked[name] = expanded


def stabilize_death_grip(armature, baked, direction):
    frames = baked['Death']
    restore(armature, frames[0])
    align_two_handed_slash(armature, direction)
    first_rotation = armature.pose.bones['R_Hand'].matrix.to_quaternion()
    blade = Vector((-.8, .6, 0))
    flat = Matrix((blade, Vector((0, 0, 1)).cross(blade), Vector((0, 0, 1)))).transposed().to_quaternion()
    wrists = []
    for pose in frames:
        restore(armature, pose)
        wrists.append(armature.pose.bones['R_Hand'].head.copy())
    for index, pose in enumerate(frames):
        restore(armature, pose)
        moment = index/BAKE_FPS
        progress = max(0, min(1, moment/1.8))
        rotation = first_rotation.slerp(flat, progress*progress*(3-2*progress)).to_matrix()
        wrist = sum((wrists[min(len(frames)-1, max(0, index+offset))] for offset in range(-6, 7)), Vector())/13
        settle = max(0, min(1, (moment-1.6)/.6))
        wrist = wrist.lerp(wrists[-1], settle*settle*(3-2*settle))
        floor_height = .012-min((rotation @ (GRIP+p)).z for p in SWORD_POINTS)
        wrist.z = max(wrist.z, floor_height)
        shoulder = armature.pose.bones['R_Upperarm'].head.copy()
        solve_arm(armature, 'R', wrist, shoulder+Vector((-.5, 0, .2)))
        hand = armature.pose.bones['R_Hand']
        hand.matrix = Matrix.Translation(wrist) @ rotation.to_4x4()
        bpy.context.view_layer.update()
        frames[index] = snapshot(armature)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--preview", action="store_true")
    args = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else [])
    directory = ROOT / "assets/skeleton_knight"
    audit = ROOT / ".codex/skeleton-knight"
    audit.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = FPS
    bpy.ops.import_scene.gltf(filepath=str(ROOT / 'client/public/models/weapons/skeleton_greatsword.glb'))
    for obj in list(bpy.data.objects):
        if obj.type == 'MESH':
            for vertex in obj.data.vertices:
                point = obj.matrix_world @ vertex.co
                SWORD_POINTS.append(Vector((point.x, point.z, -point.y)))
        bpy.data.objects.remove(obj, do_unlink=True)
    bpy.ops.import_scene.gltf(filepath=str(directory / "source.glb"))
    armature = next(obj for obj in scene.objects if obj.type == "ARMATURE")
    mesh = next(
        obj
        for obj in scene.objects
        if obj.type == "MESH" and any(modifier.type == "ARMATURE" for modifier in obj.modifiers)
    )
    for obj in list(bpy.data.objects):
        if obj not in (armature, mesh):
            bpy.data.objects.remove(obj, do_unlink=True)
    armature.animation_data_clear()
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    for bone in armature.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
        bone.rotation_mode = "QUATERNION"
    scale = HEIGHT / (
        max(vertex.co.z for vertex in mesh.data.vertices)
        - min(vertex.co.z for vertex in mesh.data.vertices)
    )
    armature.data.transform(Matrix.Scale(scale, 4))
    mesh.data.transform(Matrix.Scale(scale, 4))
    bpy.context.view_layer.update()
    armature.name, mesh.name = "SkeletonKnightRig", "SkeletonKnight"
    rest_pose = snapshot(armature)
    baked = {}
    report = {}
    for name in ["Idle", "Walk", "Run", "Attack", "Death"]:
        source_rest, frames, factor = load_motion(directory / "fbx" / f"{name}.fbx", armature)
        baked[name], report[name] = retarget(
            armature, mesh, rest_pose, source_rest, frames, factor, name
        )
        if name == 'Attack':
            source_directions = [frame['RightHand'].translation-frame['LeftHand'].translation for frame in frames]
        if name == 'Death':
            death_directions = [frame['RightHand'].translation-frame['LeftHand'].translation for frame in frames]
    weapon_motion = author_weapon_motion(armature, mesh, baked, source_directions, death_directions)
    resample_weapon_motion(armature, baked)
    stabilize_death_grip(armature, baked, death_directions[0])
    scene.render.fps = BAKE_FPS
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    actions = {}
    for name, frames in baked.items():
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        armature.animation_data_create()
        armature.animation_data.action = action
        for index, pose in enumerate(frames):
            if index:
                for bone_name in pose:
                    if pose[bone_name][1].dot(frames[index-1][bone_name][1]) < 0:
                        pose[bone_name][1].negate()
            restore(armature, pose, update=False)
            for bone in armature.pose.bones:
                for prop in ("location", "rotation_quaternion", "scale"):
                    bone.keyframe_insert(prop, frame=index, group=bone.name)
        for layer in action.layers:
            for action_strip in layer.strips:
                for bag in action_strip.channelbags:
                    for curve in bag.fcurves:
                        for key in curve.keyframe_points:
                            key.interpolation = 'LINEAR'
        track = armature.animation_data.nla_tracks.new()
        track.name = name
        strip = track.strips.new(name, 0, action)
        strip.action_slot = action.slots[0]
        strip.name = name
        track.mute = True
        actions[name] = action
    armature.animation_data.action = None
    for track in armature.animation_data.nla_tracks:
        track.mute = False
    bpy.ops.object.select_all(action="DESELECT")
    armature.select_set(True)
    mesh.select_set(True)
    bpy.context.view_layer.objects.active = armature
    scene.frame_set(0)
    scene.frame_start, scene.frame_end = 0, len(baked["Attack"]) - 1
    output = ROOT / "client/public/models/monsters/skeleton_knight.glb"
    bpy.ops.wm.save_as_mainfile(filepath=str(directory / "skeleton_knight.blend"))
    bpy.ops.export_scene.gltf(
        filepath=str(output),
        export_format="GLB",
        use_selection=True,
        export_animations=True,
        export_animation_mode="NLA_TRACKS",
        export_frame_range=False,
    )
    attack_samples = report["Attack"]["samples"]
    distances = [
        math.hypot(sample["R_Hand"][0], sample["R_Hand"][1])
        for sample in attack_samples
    ]
    impact = weapon_motion['impactSeconds']
    report['weaponMotion'] = weapon_motion
    report["impactSeconds"] = impact
    report["attackReach"] = max(distances)
    (audit / "build-report.json").write_text(json.dumps(report, indent=2) + "\n")
    summary = {
        key: {field: value for field, value in details.items() if field not in ("samples", "bladeTipSamples")}
        if isinstance(details, dict)
        else details
        for key, details in report.items()
    }
    print("FBX_BUILD", summary)
    if args.preview:
        render_previews(armature, actions, audit, impact)


if __name__ == "__main__":
    main()
