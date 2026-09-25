"""Retarget the two supplied FBX clips into a separate preview pack."""

import argparse
import json
from pathlib import Path
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import export_animations as exporter
from import_mixamo_animation import import_mixamo_animation

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path, required=True)
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:])
clips = {
    "dagger_inward": "Stable Sword Inward Slash.fbx",
    "dagger_outward": "Stable Sword Outward Slash.fbx",
}
for filename in clips.values():
    if not (args.source / filename).is_file():
        raise FileNotFoundError(args.source / filename)

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
    for curve in exporter.iter_fcurves(action):
        for key in curve.keyframe_points:
            key.co.x -= first
            key.handle_left.x -= first
            key.handle_right.x -= first
        curve.update()
    action.use_fake_user = True
    print("DAGGER_MOTION", name, json.dumps(motion))

arm.animation_data.action = None
bpy.context.scene.frame_start = 0
exporter.strip_bone_name_prefixes(arm)
exporter.rebind_action_slots_to_target(arm)
exporter.push_actions_to_nla(arm, [bpy.data.actions[name] for name in clips])
exporter.select_export_objects(arm)
exporter.export_glb(str(ROOT / "client/public/models/animations/dagger_preview.glb"))
