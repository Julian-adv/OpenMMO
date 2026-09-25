"""Author an upright enchantment hold and append it to the social pack."""

import importlib.util
import math
from pathlib import Path
import shutil
import sys
import tempfile

import bpy
from mathutils import Matrix, Quaternion, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import export_animations as exporter

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets/enchant_weapon"
PACK = ROOT / "client/public/models/animations/social.glb"
CLIPS = {"enchant_weapon": "Right", "enchant_weapon_left": "Left"}


def aim(arm, name, direction):
    bone = arm.pose.bones[name]
    current = (bone.tail - bone.head).normalized()
    rotation = current.rotation_difference(Vector(direction).normalized()).to_matrix().to_4x4()
    bone.matrix = Matrix.Translation(bone.head) @ rotation @ Matrix.Translation(-bone.head) @ bone.matrix
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

    for name, raised in CLIPS.items():
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        arm.animation_data.action = action
        for bone in arm.pose.bones:
            bone.matrix_basis = Matrix.Identity(4)
            bone.rotation_mode = "QUATERNION"
        bpy.context.view_layer.update()
        for bone in ["Spine", "Spine1", "Spine2", "Neck", "Head"]:
            aim(arm, bone, (0, 0, 1))
        for side, sign in [("Right", -1), ("Left", 1)]:
            up = side == raised
            aim(arm, side + "Shoulder", (sign, 0, 0.08 if up else -0.12))
            aim(arm, side + "Arm", (0, 0, 1) if up else (sign * 0.12, 0.025, -1))
            aim(arm, side + "ForeArm", (0, -0.025, 1) if up else (sign * 0.04, -0.025, -1))
            aim(arm, side + "Hand", (0, 0, 1) if up else (sign * 0.04, -0.025, -1))
        yaw = math.radians(-20 if raised == "Right" else 20)
        pitch = math.radians(-32)
        head = arm.pose.bones["Head"]
        head_rotation = (
            Quaternion((0, 0, 1), yaw)
            @ Quaternion((1, 0, 0), pitch)
            @ head.matrix.to_quaternion()
        )
        neck = arm.pose.bones["Neck"]
        neck_turn = (
            Quaternion((0, 0, 1), yaw * 0.25)
            @ Quaternion((1, 0, 0), pitch * 0.25)
        ).to_matrix().to_4x4()
        neck.matrix = Matrix.Translation(neck.head) @ neck_turn @ Matrix.Translation(-neck.head) @ neck.matrix
        bpy.context.view_layer.update()
        head.matrix = Matrix.LocRotScale(head.head, head_rotation, head.matrix.to_scale())
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
        track.mute = track.name != "enchant_weapon"
    scene.frame_set(0)
    bpy.context.view_layer.objects.active = arm
    arm.show_in_front = True
    bpy.ops.file.pack_all()
    bpy.ops.wm.save_as_mainfile(filepath=str(SOURCE / "enchant_weapon.blend"))


def build():
    SOURCE.mkdir(parents=True, exist_ok=True)
    baseline = SOURCE / "social-source.glb"
    if not baseline.exists():
        shutil.copy2(PACK, baseline)
    if "--export-only" in sys.argv:
        bpy.ops.wm.open_mainfile(filepath=str(SOURCE / "enchant_weapon.blend"))
    else:
        author()
    arm = bpy.data.objects["Armature"]
    for track in arm.animation_data.nla_tracks:
        track.mute = False
    exporter.select_export_objects(arm)
    spec = importlib.util.spec_from_file_location("graft", ROOT / "tools/graft-glb-clip.py")
    graft = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(graft)
    with tempfile.TemporaryDirectory(prefix="enchant-animation-") as temporary:
        donor = Path(temporary) / "enchant.glb"
        exporter.export_glb(str(donor))
        target, blob = graft.read_glb(baseline)
        target["animations"] = [a for a in target["animations"] if a["name"] not in CLIPS]
        current_pack, _ = graft.read_glb(PACK)
        current_clips = [a for a in current_pack["animations"] if a["name"] not in CLIPS]
        if current_clips != target["animations"] or current_pack["nodes"] != target["nodes"]:
            raise RuntimeError("Social pack changed; refresh social-source.glb before exporting")
        current = Path(temporary) / "social-base.glb"
        graft.write_glb(current, target, blob)
        for index, name in enumerate(CLIPS):
            output = Path(temporary) / f"social-{index}.glb"
            graft.graft(current, donor, name, output)
            current = output
        PACK.write_bytes(current.read_bytes())


if __name__ == "__main__":
    build()
