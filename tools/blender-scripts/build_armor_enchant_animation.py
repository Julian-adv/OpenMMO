"""Author the open-palm armor enchantment hold."""

import math
from pathlib import Path
import sys

import bpy
from mathutils import Matrix, Quaternion, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import export_animations as exporter
from build_enchant_animation import aim

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/enchant_armor/enchant_armor.blend"
PACK = ROOT / "client/public/models/animations/enchant_armor.glb"
CLIPS = {"enchant_armor": "Left", "enchant_armor_left": "Right"}


def open_palm(arm, side, sign):
    hand = arm.pose.bones[side + "Hand"]
    direction = Vector((sign * 0.8, -0.5, 0.1)).normalized()
    normal = Vector((0, 0, 1))
    normal = (normal - direction * normal.dot(direction)).normalized()
    across = direction.cross(normal).normalized()
    rotation = Matrix((across, direction, normal)).transposed().to_quaternion()
    hand.matrix = Matrix.LocRotScale(hand.head, rotation, hand.matrix.to_scale())
    bpy.context.view_layer.update()


def author():
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / "assets/all_animation.blend"))
    arm = bpy.data.objects["Armature"]
    keep = {arm, *arm.children_recursive}
    for obj in list(bpy.data.objects):
        if obj not in keep:
            bpy.data.objects.remove(obj, do_unlink=True)
        else:
            obj.animation_data_clear()
    for action in list(bpy.data.actions):
        bpy.data.actions.remove(action)
    for material in list(bpy.data.materials):
        bpy.data.materials.remove(material)
    for image in list(bpy.data.images):
        bpy.data.images.remove(image)
    bpy.data.orphans_purge(do_recursive=True)
    arm.animation_data_create()
    scene = bpy.context.scene
    scene.render.fps = 30
    scene.frame_start = 0
    scene.frame_end = 120
    for name, receiving in CLIPS.items():
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        arm.animation_data.action = action
        for bone in arm.pose.bones:
            bone.matrix_basis = Matrix.Identity(4)
            bone.rotation_mode = "QUATERNION"
        bpy.context.view_layer.update()
        for name in ("Spine", "Spine1", "Spine2", "Neck", "Head"):
            aim(arm, name, (0, 0, 1))
        for side, sign in (("Right", -1), ("Left", 1)):
            opened = side == receiving
            aim(arm, side + "Shoulder", (sign, 0, -0.1))
            aim(arm, side + "Arm", (sign * 0.45, 0, -1))
            aim(arm, side + "ForeArm", (sign * 0.8, -0.5, -0.25) if opened else (sign * 0.7, -0.35, -0.4))
            if opened:
                open_palm(arm, side, sign)
            else:
                aim(arm, side + "Hand", (sign * 0.6, -0.3, -0.45))
        for name, weight in (("Neck", 0.3), ("Head", 0.7)):
            bone = arm.pose.bones[name]
            yaw = math.radians(16 if receiving == "Left" else -16) * weight
            pitch = math.radians(12) * weight
            turn = (Quaternion((0, 0, 1), yaw) @ Quaternion((1, 0, 0), pitch)).to_matrix().to_4x4()
            bone.matrix = Matrix.Translation(bone.head) @ turn @ Matrix.Translation(-bone.head) @ bone.matrix
            bpy.context.view_layer.update()
        for frame in (0, 120):
            for bone in arm.pose.bones:
                bone.keyframe_insert("location", frame=frame)
                bone.keyframe_insert("rotation_quaternion", frame=frame)
                bone.keyframe_insert("scale", frame=frame)
        for curve in exporter.iter_fcurves(action):
            for point in curve.keyframe_points:
                point.interpolation = "LINEAR"
        arm.animation_data.action = None
    exporter.rebind_action_slots_to_target(arm)
    exporter.push_actions_to_nla(arm, [bpy.data.actions[name] for name in CLIPS])
    exporter.select_export_objects(arm)
    for track in arm.animation_data.nla_tracks:
        track.mute = track.name != "enchant_armor"
    scene.frame_set(0)
    bpy.context.view_layer.objects.active = arm
    arm.show_in_front = True
    bpy.ops.file.pack_all()
    bpy.ops.wm.save_as_mainfile(filepath=str(SOURCE))


def build():
    SOURCE.parent.mkdir(parents=True, exist_ok=True)
    if "--export-only" in sys.argv:
        bpy.ops.wm.open_mainfile(filepath=str(SOURCE))
    else:
        author()
    arm = bpy.data.objects["Armature"]
    for track in arm.animation_data.nla_tracks:
        track.mute = False
    exporter.select_export_objects(arm)
    exporter.export_glb(str(PACK))


if __name__ == "__main__":
    build()
