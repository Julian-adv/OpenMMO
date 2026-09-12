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
HEIGHT = 1.9
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
        times = [impact - 0.25, impact, impact + 0.25] if name == "Attack" else [action.frame_range[1] / FPS if name == "Death" else 0.2]
        for moment in times:
            scene.frame_set(int(moment * FPS), subframe=(moment * FPS) % 1)
            scene.render.filepath = str(directory / f"{name}-{round(moment * 1000):04d}.png")
            bpy.ops.render.render(write_still=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--preview", action="store_true")
    args = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else [])
    directory = ROOT / "assets/skeleton_warrior"
    audit = ROOT / ".codex/skeleton-warrior"
    audit.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = FPS
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
    armature.name, mesh.name = "SkeletonWarriorRig", "SkeletonWarrior"
    rest_pose = snapshot(armature)
    baked = {}
    report = {}
    for name in ["Idle", "Walk", "Run", "Attack", "Death"]:
        source_rest, frames, factor = load_motion(directory / "fbx" / f"{name}.fbx", armature)
        baked[name], report[name] = retarget(
            armature, mesh, rest_pose, source_rest, frames, factor, name
        )
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    actions = {}
    for name, frames in baked.items():
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        armature.animation_data_create()
        armature.animation_data.action = action
        for index, pose in enumerate(frames):
            restore(armature, pose, update=False)
            for bone in armature.pose.bones:
                for prop in ("location", "rotation_quaternion", "scale"):
                    bone.keyframe_insert(prop, frame=index, group=bone.name)
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
    output = ROOT / "client/public/models/monsters/skeleton_warrior.glb"
    bpy.ops.wm.save_as_mainfile(filepath=str(directory / "skeleton_warrior.blend"))
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
    impact = max(range(len(distances)), key=distances.__getitem__) / FPS
    report["impactSeconds"] = impact
    report["attackReach"] = max(distances)
    (audit / "build-report.json").write_text(json.dumps(report, indent=2) + "\n")
    summary = {
        key: {field: value for field, value in details.items() if field != "samples"}
        if isinstance(details, dict)
        else details
        for key, details in report.items()
    }
    print("FBX_BUILD", summary)
    if args.preview:
        render_previews(armature, actions, audit, impact)


if __name__ == "__main__":
    main()
