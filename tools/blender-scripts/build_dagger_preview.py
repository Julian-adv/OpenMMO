"""Retarget supplied Mixamo FBX clips into a weapon animation pack.

blender -b --python tools/blender-scripts/build_dagger_preview.py -- --source assets/animations/dagger [--pack spear]
"""

import argparse
import json
import math
from pathlib import Path
import sys

import bpy
from mathutils import Matrix, Quaternion, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import export_animations as exporter
from import_mixamo_animation import import_mixamo_animation

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path, required=True)
PACKS = {
    "dagger_preview": {
        "dagger_inward": "Stable Sword Inward Slash.fbx",
        "dagger_outward": "Stable Sword Outward Slash.fbx",
        "dagger_stab": "Stabbing.fbx",
    },
    "spear": {"spear_thrust": "Bayonet Stab.fbx"},
}
# Clips derived from an action already in the blend file instead of an FBX.
DERIVED = {"spear": {"spear_idle": "idle1", "spear_walk": "walk", "spear_run": "run"}}
# At rest the spear stands upright: the upper arm hangs, the elbow bends to a right angle with the forearm
# level and forward, and the hand turns the shaft (the hand bone's X) to vertical.
REST_UPPER = Vector((-0.12, 0.05, -1))
REST_FOREARM = math.radians(0)
# On the move the upper arm swings forward to open the elbow, the forearm lifts just enough for the butt to clear
# the ground, and the shaft leans forward. All hold still in the world while the body swings under them.
CARRY_UPPER = Vector((-0.12, -math.sin(math.radians(20)), -math.cos(math.radians(20))))
CARRY_FOREARM = math.radians(18)
# A walking shoulder dips lower than a running one.
WALK_FOREARM = math.radians(28)
CARRY_LEAN = math.radians(12)
parser.add_argument("--pack", choices=PACKS, default="dagger_preview")
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:])
clips = PACKS[args.pack]
for filename in clips.values():
    if not (args.source / filename).is_file():
        raise FileNotFoundError(args.source / filename)

# Mixamo's stab rises above the shoulder; aim it level at chest height instead.
STAB_ELEVATION = math.radians(-8)
STAB_WRIST = math.radians(-15)
# Cut most of the frozen full-extension pose so the recovery fits a 1.2 s swing.
STAB_HOLD = (17, 26)


def rotate_about_head(arm, bone, rotation):
    world = arm.matrix_world.to_quaternion()
    local = (world.inverted() @ rotation @ world).to_matrix().to_4x4()
    head = bone.head.copy()
    bone.matrix = Matrix.Translation(head) @ local @ Matrix.Translation(-head) @ bone.matrix
    bpy.context.view_layer.update()


def level_stab(arm, action):
    scene = bpy.context.scene
    upper, hand = arm.pose.bones["RightArm"], arm.pose.bones["RightHand"]
    first, last = map(int, action.frame_range)
    hands = {}
    for frame in range(first, last + 1):
        scene.frame_set(frame)
        hands[frame] = arm.matrix_world @ hand.head
    peak = max(hands, key=lambda f: (hands[f] - hands[first]).xy.length)
    forward = hands[peak] - hands[first]
    forward.z = 0
    reach = forward.length
    forward.normalize()
    side = forward.cross(Vector((0, 0, 1)))
    previous = {}
    for frame in range(first, last + 1):
        scene.frame_set(frame)
        progress = (hands[frame] - hands[first]).dot(forward) / reach
        t = min(max((progress - 0.3) / 0.6, 0), 1)
        weight = t * t * (3 - 2 * t)
        if weight > 0:
            reach_vector = arm.matrix_world @ hand.head - arm.matrix_world @ upper.head
            elevation = math.asin(reach_vector.z / reach_vector.length)
            rotate_about_head(arm, upper, Quaternion(side, (STAB_ELEVATION - elevation) * weight))
            rotate_about_head(arm, hand, Quaternion(side, STAB_WRIST * weight))
        for bone in (upper, hand):
            if bone.name in previous:
                bone.rotation_quaternion.make_compatible(previous[bone.name])
            previous[bone.name] = bone.rotation_quaternion.copy()
            bone.keyframe_insert("rotation_quaternion", frame=frame, group=bone.name)


# Mixamo's bayonet stab drives the point up to the chest; both hands come down this far (m) at full reach
# for a thrust at the belly.
THRUST_DROP = 0.17
# The wind-up draws both hands this much farther back (m) at its deepest (user request 2026-11-18).
THRUST_PULL = 0.18
# Source frames kept, from the start of the wind-up to the return to guard, and the playback speed-up that
# lands full reach on the shared 540 ms impact.
THRUST_FRAMES = (9, 65)
THRUST_SPEED = 0.54


def lower_thrust(arm, action):
    scene = bpy.context.scene
    arms = [(arm.pose.bones[side + "Arm"], arm.pose.bones[side + "Hand"]) for side in ("Right", "Left")]
    lead = arms[0][1]
    first, last = map(int, action.frame_range)
    hands = {}
    for frame in range(first, last + 1):
        scene.frame_set(frame)
        hands[frame] = arm.matrix_world @ lead.head
    peak = max(hands, key=lambda f: (hands[f] - hands[first]).xy.length)
    forward = hands[peak] - hands[first]
    forward.z = 0
    reach = forward.length
    forward.normalize()
    side = forward.cross(Vector((0, 0, 1)))
    deepest = max(-(hands[f] - hands[first]).dot(forward) for f in hands)
    previous = {}
    for frame in range(first, last + 1):
        scene.frame_set(frame)
        progress = (hands[frame] - hands[first]).dot(forward)
        t = min(max(progress / reach, 0), 1)
        weight = t * t * (3 - 2 * t)
        b = min(max(-progress / deepest, 0), 1) if deepest > 0 else 0
        pull = THRUST_PULL * b * b * (3 - 2 * b)
        for upper, hand in arms:
            if pull > 0:
                span = arm.matrix_world @ hand.head - arm.matrix_world @ upper.head
                rotate_about_head(arm, upper, span.rotation_difference(span - forward * pull))
            if weight > 0:
                span = arm.matrix_world @ hand.head - arm.matrix_world @ upper.head
                target = max(min((span.z - THRUST_DROP * weight) / span.length, 1), -1)
                rotate_about_head(arm, upper, Quaternion(side, math.asin(target) - math.asin(span.z / span.length)))
            if upper.name in previous:
                upper.rotation_quaternion.make_compatible(previous[upper.name])
            previous[upper.name] = upper.rotation_quaternion.copy()
            upper.keyframe_insert("rotation_quaternion", frame=frame, group=upper.name)


def retime(action, first, last, speed):
    """Keep source frames first..last and play them `speed` times as long."""
    for curve in exporter.iter_fcurves(action):
        for key in reversed(list(curve.keyframe_points)):
            if not first <= key.co.x <= last:
                curve.keyframe_points.remove(key)
        for key in curve.keyframe_points:
            for point in (key.co, key.handle_left, key.handle_right):
                point.x = first + (point.x - first) * speed
        curve.update()


def aim(arm, bone, axis, direction):
    current = arm.matrix_world.to_3x3() @ bone.matrix.to_3x3() @ axis
    rotate_about_head(arm, bone, current.rotation_difference(direction))


def shoulder_spear(arm, source, name):
    """Copy a clip and hold the spear in the folded right arm through it: upright at rest, leaning on the move."""
    moving = name != "spear_idle"
    fold = (WALK_FOREARM if name == "spear_walk" else CARRY_FOREARM) if moving else REST_FOREARM
    lean = CARRY_LEAN if moving else 0
    action = source.copy()
    action.name = name
    arm.animation_data.action = action
    arm.animation_data.action_slot = action.slots[0]
    upper, fore, hand = (arm.pose.bones["Right" + part] for part in ("Arm", "ForeArm", "Hand"))
    along = Vector((0, 1, 0))
    lift = Vector((0, -math.cos(fold), math.sin(fold)))
    shaft = Vector((0, -math.sin(lean), math.cos(lean)))
    first, last = map(int, action.frame_range)
    previous = {}
    for frame in range(first, last + 1):
        bpy.context.scene.frame_set(frame)
        aim(arm, upper, along, CARRY_UPPER if moving else REST_UPPER)
        aim(arm, fore, along, lift)
        aim(arm, hand, Vector((1, 0, 0)), shaft)
        # Roll the hand about the shaft until it points where the forearm does.
        pointing = arm.matrix_world.to_3x3() @ hand.matrix.to_3x3() @ along
        flat = [(v - shaft * v.dot(shaft)).normalized() for v in (pointing, lift)]
        rotate_about_head(arm, hand, flat[0].rotation_difference(flat[1]))
        for bone in (upper, fore, hand):
            if bone.name in previous:
                bone.rotation_quaternion.make_compatible(previous[bone.name])
            previous[bone.name] = bone.rotation_quaternion.copy()
            bone.keyframe_insert("rotation_quaternion", frame=frame, group=bone.name)
    for curve in exporter.iter_fcurves(action):
        for key in curve.keyframe_points:
            for point in (key.co, key.handle_left, key.handle_right):
                point.x = (point.x - first) * 24 / 30
        curve.update()
    action.use_fake_user = True
    return action


def cut_frames(action, first, last):
    removed = last - first + 1
    for curve in exporter.iter_fcurves(action):
        for key in reversed(list(curve.keyframe_points)):
            if first <= key.co.x <= last:
                curve.keyframe_points.remove(key)
        for key in curve.keyframe_points:
            if key.co.x > last:
                key.co.x -= removed
                key.handle_left.x -= removed
                key.handle_right.x -= removed
        curve.update()


bpy.ops.wm.open_mainfile(filepath=str(ROOT / "assets/all_animation.blend"))
arm = bpy.data.objects["Armature_combat"]
exporter.clear_nla_tracks(arm)
arm.animation_data.action = None

for name, filename in clips.items():
    bpy.context.scene.render.fps = 30
    import_mixamo_animation(str(args.source / filename), name, arm.name, save=False)
    action = bpy.data.actions[name]
    arm.animation_data.action = action
    arm.animation_data.action_slot = action.slots[0]
    if name == "dagger_stab":
        level_stab(arm, action)
    if name == "spear_thrust":
        lower_thrust(arm, action)
    hips = arm.pose.bones["Hips"]
    first, last = map(int, action.frame_range)
    bpy.context.scene.frame_set(first)
    origin = (arm.matrix_world @ hips.matrix).translation.copy()
    axes = arm.matrix_world.to_3x3() @ hips.bone.matrix_local.to_3x3()
    samples = []
    motion = []
    for frame in range(first, last + 1):
        bpy.context.scene.frame_set(frame)
        displacement = (arm.matrix_world @ hips.matrix).translation - origin
        correction = axes.inverted() @ Vector((displacement.x, displacement.y, 0))
        samples.append((frame, hips.location - correction))
        hand = (arm.matrix_world @ arm.pose.bones["RightHand"].matrix).translation
        if (frame - first) % 3 == 0:
            motion.append([frame - first, *[round(v, 3) for v in hand]])
    for frame, position in samples:
        hips.location = position
        hips.keyframe_insert(data_path="location", frame=frame, group="Hips")
    if name == "dagger_stab":
        cut_frames(action, first + STAB_HOLD[0], first + STAB_HOLD[1])
    if name == "spear_thrust":
        retime(action, first + THRUST_FRAMES[0] - 1, first + THRUST_FRAMES[1] - 1, THRUST_SPEED)
        first += THRUST_FRAMES[0] - 1
    for curve in exporter.iter_fcurves(action):
        for key in curve.keyframe_points:
            key.co.x -= first
            key.handle_left.x -= first
            key.handle_right.x -= first
        curve.update()
    action.use_fake_user = True
    print("DAGGER_MOTION", name, json.dumps(motion))

derived = [shoulder_spear(arm, bpy.data.actions[source], name)
           for name, source in DERIVED.get(args.pack, {}).items()]
arm.animation_data.action = None
# The shipped slash clips and their combo windows are timed at 24 fps.
bpy.context.scene.render.fps = 24
bpy.context.scene.frame_start = 0
exporter.strip_bone_name_prefixes(arm)
exporter.rebind_action_slots_to_target(arm)
exporter.push_actions_to_nla(arm, [bpy.data.actions[name] for name in clips] + derived)
exporter.select_export_objects(arm)
exporter.export_glb(str(ROOT / f"client/public/models/animations/{args.pack}.glb"))
