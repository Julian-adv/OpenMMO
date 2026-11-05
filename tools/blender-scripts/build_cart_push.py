"""Build the ore-cart push pack from Mixamo's Push Stop."""

from pathlib import Path
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import export_animations as exporter
from import_mixamo_animation import import_mixamo_animation

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/animations/push/Push Stop.fbx"
CLIP = "cart_push"
# Frames before this are a push-walk that treads in place once root motion is
# stripped; keep the last shove, the release and the stand-up.
FIRST_FRAME = 45

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
# Measured from the clip's start so the push-walk's travel is removed too.
bpy.context.scene.frame_set(first)
origin = (arm.matrix_world @ hips.matrix).translation.copy()
local_axes = arm.matrix_world.to_3x3() @ hips.bone.matrix_local.to_3x3()
samples = []
for frame in range(FIRST_FRAME, last + 1):
    bpy.context.scene.frame_set(frame)
    displacement = (arm.matrix_world @ hips.matrix).translation - origin
    correction = local_axes.inverted() @ Vector((displacement.x, displacement.y, 0))
    samples.append((frame, hips.location - correction))
for frame, position in samples:
    hips.location = position
    hips.keyframe_insert(data_path="location", frame=frame, group="Hips")
for curve in exporter.iter_fcurves(action):
    for key in reversed(list(curve.keyframe_points)):
        if key.co.x < FIRST_FRAME:
            curve.keyframe_points.remove(key)
    for key in curve.keyframe_points:
        key.co.x -= FIRST_FRAME
        key.handle_left.x -= FIRST_FRAME
        key.handle_right.x -= FIRST_FRAME
    curve.update()
action.use_fake_user = True
print("CART_PUSH_CLIP", (last - FIRST_FRAME) / bpy.context.scene.render.fps)

arm.animation_data.action = None
bpy.context.scene.frame_start = 0
exporter.strip_bone_name_prefixes(arm)
exporter.rebind_action_slots_to_target(arm)
exporter.push_actions_to_nla(arm, [action])
exporter.select_export_objects(arm)
exporter.export_glb(str(ROOT / "client/public/models/animations/cart_push.glb"))
