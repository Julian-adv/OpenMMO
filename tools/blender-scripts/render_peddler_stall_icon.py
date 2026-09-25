"""Render the Peddler's Stall inventory icon from the stall table GLB.

The icon is the same table the item lays out (black_market_table.glb), shot
under the shared icon recipe in .claude/skills/blender-item-asset, so what the
bag shows and what the ground grows are one model.

    /Applications/Blender.app/Contents/MacOS/Blender -b \
        -P tools/blender-scripts/render_peddler_stall_icon.py
"""

import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from icon_render import add_light, render_icon  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MODEL = os.path.join(REPO, "client/public/models/objects/black_market_table.glb")
ICON = os.path.join(REPO, "client/public/items/objects/peddler_stall.png")


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=MODEL)
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if not meshes:
        raise SystemExit(f"no mesh in {MODEL}")

    add_light((0.9, -1.2, 1.6), 320.0, 2.0)
    add_light((-1.4, -0.6, 0.9), 120.0, 2.4)
    add_light((0.0, 1.6, 1.1), 90.0, 2.4)

    # Tipped toward the camera so the wares read as goods rather than a flat
    # plank, but square to the frame — no roll, so the table sits level in the
    # bag slot instead of leaning across it.
    render_icon(
        meshes,
        ICON,
        (math.radians(-62), 0.0, 0.0),
    )


main()
