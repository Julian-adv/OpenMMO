"""Build the great-sword pack from the four user-supplied FBX clips."""

from pathlib import Path
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import export_animations as exporter
from import_mixamo_animation import import_mixamo_animation

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/great_sword/animations"
CLIPS = {
    "great_sword_idle": "Great Sword Idle (1).fbx",
    "great_sword_walk": "Great Sword Walk.fbx",
    "great_sword_run": "Great Sword Run.fbx",
    "great_sword_slash": "Great Sword Slash.fbx",
}

bpy.ops.wm.open_mainfile(filepath=str(ROOT / "assets/all_animation.blend"))
arm = bpy.data.objects["Armature_combat"]
exporter.clear_nla_tracks(arm)
arm.animation_data.action = None

for name, filename in CLIPS.items():
    bpy.context.scene.render.fps = 30
    import_mixamo_animation(str(SOURCE / filename), name, arm.name, save=False)
    action = bpy.data.actions[name]
    arm.animation_data.action = action
    arm.animation_data.action_slot = action.slots[0]
    hips = arm.pose.bones["Hips"]
    first, last = map(int, action.frame_range)
    bpy.context.scene.frame_set(first)
    origin = (arm.matrix_world @ hips.matrix).translation.copy()
    local_axes = arm.matrix_world.to_3x3() @ hips.bone.matrix_local.to_3x3()
    samples = []
    for frame in range(first, last + 1):
        bpy.context.scene.frame_set(frame)
        displacement = (arm.matrix_world @ hips.matrix).translation - origin
        correction = local_axes.inverted() @ Vector((displacement.x, displacement.y, 0))
        samples.append((frame, hips.location - correction))
    for frame, position in samples:
        hips.location = position
        hips.keyframe_insert(data_path="location", frame=frame, group="Hips")
    for curve in exporter.iter_fcurves(action):
        for key in curve.keyframe_points:
            key.co.x -= first
            key.handle_left.x -= first
            key.handle_right.x -= first
            if name == "great_sword_walk":
                left = key.handle_left.copy()
                right = key.handle_right.copy()
                key.co.x = last - first - key.co.x
                key.handle_left = (last - first - right.x, right.y)
                key.handle_right = (last - first - left.x, left.y)
        curve.update()
    print("GREAT_SWORD_CLIP", name, (last - first) / bpy.context.scene.render.fps)

arm.animation_data.action = None
bpy.context.scene.frame_start = 0
exporter.strip_bone_name_prefixes(arm)
exporter.rebind_action_slots_to_target(arm)
exporter.push_actions_to_nla(arm, [bpy.data.actions[name] for name in CLIPS])
exporter.select_export_objects(arm)
exporter.export_glb(str(ROOT / "client/public/models/animations/great_sword.glb"))
bpy.ops.wm.save_as_mainfile(filepath=str(SOURCE / "great_sword_animations.blend"))
