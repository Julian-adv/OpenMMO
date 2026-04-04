"""
Blender script to clean up a mesh and export as FBX (no skeleton).

Steps:
  1. Merge duplicate vertices (by distance)
  2. Apply smooth shading
  3. Export selected mesh as FBX (mesh only, no armature)

Usage:
  - Select the mesh object in Blender
  - Run this script from the Text Editor (Alt+P)
  - The FBX is saved next to the .blend file as <object_name>.fbx
"""

import bpy
import os

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

MERGE_THRESHOLD = 0.0001  # Distance threshold for merging vertices


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    obj = bpy.context.active_object
    if not obj or obj.type != "MESH":
        print("ERROR: No active mesh object selected.")
        return

    print(f"Processing: {obj.name}")
    print(f"  Vertices before: {len(obj.data.vertices)}")

    # 1. Merge duplicate vertices
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.remove_doubles(threshold=MERGE_THRESHOLD)
    bpy.ops.object.mode_set(mode="OBJECT")

    print(f"  Vertices after merge: {len(obj.data.vertices)}")

    # 2. Smooth shading
    bpy.ops.object.shade_smooth()
    print("  Smooth shading applied")

    # 3. Export as FBX (mesh only)
    blend_dir = os.path.dirname(bpy.data.filepath) or "."
    output = os.path.join(blend_dir, f"{obj.name}.fbx")

    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj

    bpy.ops.export_scene.fbx(
        filepath=output,
        use_selection=True,
        object_types={"MESH"},
        add_leaf_bones=False,
    )

    print(f"  Exported to: {output}")


if __name__ == "__main__":
    main()
