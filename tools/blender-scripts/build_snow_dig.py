"""Build the snow-clearing pack from Mixamo's Digging."""

from pathlib import Path
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import export_animations as exporter
from import_mixamo_animation import import_mixamo_animation

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/animations/shovel/Digging.fbx"
CLIP = "snow_dig"

bpy.ops.wm.open_mainfile(filepath=str(ROOT / "assets/all_animation.blend"))
arm = bpy.data.objects["Armature_combat"]
exporter.clear_nla_tracks(arm)
arm.animation_data.action = None

bpy.context.scene.render.fps = 30
import_mixamo_animation(str(SOURCE), CLIP, arm.name, save=False)
action = bpy.data.actions[CLIP]
arm.animation_data.action = action
arm.animation_data.action_slot = action.slots[0]
hips = arm.pose.bones["Hips"]
first, last = map(int, action.frame_range)
bpy.context.scene.frame_set(first)
origin = (arm.matrix_world @ hips.matrix).translation.copy()
local_axes = arm.matrix_world.to_3x3() @ hips.bone.matrix_local.to_3x3()
samples = []
drift = 0.0
for frame in range(first, last + 1):
    bpy.context.scene.frame_set(frame)
    displacement = (arm.matrix_world @ hips.matrix).translation - origin
    drift = max(drift, Vector((displacement.x, displacement.y)).length)
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
    curve.update()
action.use_fake_user = True
print("SNOW_DIG_CLIP", first, last, (last - first) / bpy.context.scene.render.fps, "drift", drift)

arm.animation_data.action = None
bpy.context.scene.frame_start = 0
exporter.strip_bone_name_prefixes(arm)
exporter.rebind_action_slots_to_target(arm)
exporter.push_actions_to_nla(arm, [action])
exporter.select_export_objects(arm)
exporter.export_glb(str(ROOT / "client/public/models/animations/snow_dig.glb"))
