"""
Blender script to selectively export animations from all_animation.blend
into separate GLB files by category.

Usage (from project root):
  blender assets/all_animation.blend --background --python tools/blender-scripts/export_animations.py

Or run from Blender's Text Editor for interactive use.
"""

import bpy
import os

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

OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "client", "public", "models", "animations")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def get_armature():
    """Find the first armature object in the scene."""
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


def ensure_armature_has_mesh(armature):
    """Ensure the armature has at least one child mesh for skeleton export.

    The glTF exporter only writes skin/skeleton data when a SkinnedMesh is
    present. Without it, the exported GLB has no skeleton and runtime
    retargeting (which needs a source SkinnedMesh) silently fails.
    If no mesh exists, create a minimal single-vertex mesh weighted to the
    armature so the skeleton is included in the export.

    Returns the created mesh object, or None if a mesh already existed.
    """
    for obj in bpy.data.objects:
        if obj.type == "MESH" and obj.parent == armature:
            return None

    # Create minimal mesh
    mesh_data = bpy.data.meshes.new("_export_helper_mesh")
    mesh_data.from_pydata([(0, 0, 0)], [], [])
    mesh_data.update()

    mesh_obj = bpy.data.objects.new("_export_helper", mesh_data)
    bpy.context.scene.collection.objects.link(mesh_obj)
    mesh_obj.parent = armature

    # Add Armature modifier so the mesh is bound to the skeleton
    mod = mesh_obj.modifiers.new(name="Armature", type="ARMATURE")
    mod.object = armature

    # Add a vertex group for the root bone so glTF sees it as a skinned mesh
    if armature.data.bones:
        root_bone = next((b for b in armature.data.bones if b.parent is None), armature.data.bones[0])
        vg = mesh_obj.vertex_groups.new(name=root_bone.name)
        vg.add([0], 1.0, "REPLACE")

    print(f"  Created helper mesh bound to '{armature.name}' for skeleton export")
    return mesh_obj


def cleanup_helper_mesh(mesh_obj):
    """Remove the temporary helper mesh created by ensure_armature_has_mesh."""
    if mesh_obj is None:
        return
    mesh_data = mesh_obj.data
    bpy.data.objects.remove(mesh_obj, do_unlink=True)
    bpy.data.meshes.remove(mesh_data)


def export_glb(filepath):
    """Export the current scene as GLB with skeleton data."""
    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=filepath,
        export_format="GLB",
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

    all_actions = collect_all_actions()
    print(f"Found {len(all_actions)} actions: {list(all_actions.keys())}")

    # Ensure a SkinnedMesh exists so the glTF exporter includes skeleton data.
    # This is required for runtime retargeting to work across different character models.
    helper_mesh = ensure_armature_has_mesh(armature)

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

        output_path = os.path.join(OUTPUT_DIR, f"{pack_name}.glb")
        print(f"Exporting {len(actions_to_export)} animations to: {output_path}")
        for action in actions_to_export:
            print(f"  - {action.name} ({int(action.frame_range[1] - action.frame_range[0])} frames)")

        export_glb(output_path)
        print(f"Done: {output_path}")

    # Clean up
    cleanup_helper_mesh(helper_mesh)
    clear_nla_tracks(armature)
    print("\nAll packs exported successfully.")


if __name__ == "__main__":
    main()
