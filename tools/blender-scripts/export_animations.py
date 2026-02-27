"""
Blender script to selectively export animations from all_animation.blend
into separate GLB files by category.

Usage (from project root):
  blender assets/all_animation.blend --background --python tools/blender-scripts/export_animations.py

Or run from Blender's Text Editor for interactive use.
"""

import bpy
import os
import re

# ---------------------------------------------------------------------------
# Configuration: define which actions go into which GLB file.
# Action names must match the names in the Blender file exactly.
# ---------------------------------------------------------------------------

EXPORT_PACKS = {
    "locomotion": [
        "idle1",
        "idle2",
        "idle3",
        "idle4",
        "idle5",
        "walk",
        "jog",
        "run",
    ],
    "combat_melee": [
        "slash1",
        "slash2",
        "slash3",
        "slash4",
        "slash5",
        "attack1",
        "attack2",
        "attack3",
        "attack4",
        "dying",
    ],
}

# The primary armature name whose mesh and skeleton should be exported.
# Other armatures (e.g. "Armature.001") will be excluded.
EXPORT_ARMATURE_NAME = "Armature"

OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "client", "public", "models", "animations")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def get_armature():
    """Find the armature to export by EXPORT_ARMATURE_NAME."""
    arm = bpy.data.objects.get(EXPORT_ARMATURE_NAME)
    if arm and arm.type == "ARMATURE":
        return arm
    # Fallback: first armature in the scene
    for obj in bpy.data.objects:
        if obj.type == "ARMATURE":
            return obj
    return None


def collect_all_actions():
    """Return a dict of action_name -> action for all actions in the file."""
    return {action.name: action for action in bpy.data.actions}


def clear_nla_tracks(armature):
    """Remove all NLA tracks from the armature."""
    if armature.animation_data is None:
        armature.animation_data_create()
    tracks = armature.animation_data.nla_tracks
    while len(tracks) > 0:
        tracks.remove(tracks[0])


def push_actions_to_nla(armature, actions):
    """Push a list of actions as NLA strips on separate tracks."""
    anim_data = armature.animation_data
    for action in actions:
        track = anim_data.nla_tracks.new()
        track.name = action.name
        strip = track.strips.new(action.name, int(action.frame_range[0]), action)
        strip.name = action.name


def strip_bone_name_prefixes(armature):
    """Remove Mixamo prefixes (e.g. 'mixamorig:') from bone names.

    Also updates animation F-curve data_paths that reference bone names.
    Changes persist in the .blend file if saved afterwards.
    """
    prefix_pattern = re.compile(r'^mixamorig\d*:')
    original_to_new = {}

    for bone in armature.data.bones:
        new_name = prefix_pattern.sub("", bone.name)
        if new_name != bone.name:
            original_to_new[bone.name] = new_name
            bone.name = new_name

    if not original_to_new:
        print("  No bone prefixes to strip")
        return

    # Update F-curve data_paths to match renamed bones
    for action in bpy.data.actions:
        for fc in action.fcurves:
            for orig, new in original_to_new.items():
                if orig in fc.data_path:
                    fc.data_path = fc.data_path.replace(
                        f'["{orig}"]', f'["{new}"]'
                    )

    print(f"  Stripped prefixes from {len(original_to_new)} bones")


def select_export_objects(armature):
    """Select only the target armature and its child meshes for export."""
    bpy.ops.object.select_all(action="DESELECT")
    armature.select_set(True)
    for obj in bpy.data.objects:
        if obj.type == "MESH" and obj.parent == armature:
            obj.select_set(True)
            print(f"  Including mesh: {obj.name}")


def export_glb(filepath):
    """Export selected objects as GLB with skeleton data."""
    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=filepath,
        export_format="GLB",
        use_selection=True,
        export_animations=True,
        export_skins=True,
        export_nla_strips=True,
        export_nla_strips_merged_animation_name="",
        export_animation_mode="NLA_TRACKS",
    )


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    armature = get_armature()
    if armature is None:
        print("ERROR: No armature found in the scene.")
        return

    print(f"Using armature: '{armature.name}'")

    # Standardize bone names before export
    strip_bone_name_prefixes(armature)

    all_actions = collect_all_actions()
    print(f"Found {len(all_actions)} actions: {list(all_actions.keys())}")

    os.makedirs(OUTPUT_DIR, exist_ok=True)

    for pack_name, action_names in EXPORT_PACKS.items():
        print(f"\n--- Exporting pack: {pack_name} ---")

        # Validate that all requested actions exist
        missing = [name for name in action_names if name not in all_actions]
        if missing:
            print(f"WARNING: Missing actions for {pack_name}: {missing}")

        actions_to_export = [all_actions[name] for name in action_names if name in all_actions]
        if not actions_to_export:
            print(f"SKIPPED: No actions found for {pack_name}")
            continue

        # Set up NLA tracks with only the desired actions
        clear_nla_tracks(armature)
        push_actions_to_nla(armature, actions_to_export)

        # Clear the active action so it doesn't get exported as an extra clip
        armature.animation_data.action = None

        # Select only the target armature and its child meshes (excludes Armature.001 etc.)
        select_export_objects(armature)

        output_path = os.path.join(OUTPUT_DIR, f"{pack_name}.glb")
        print(f"Exporting {len(actions_to_export)} animations to: {output_path}")
        for action in actions_to_export:
            print(f"  - {action.name} ({int(action.frame_range[1] - action.frame_range[0])} frames)")

        export_glb(output_path)
        print(f"Done: {output_path}")

    # Clean up
    clear_nla_tracks(armature)
    print("\nAll packs exported successfully.")


if __name__ == "__main__":
    main()
