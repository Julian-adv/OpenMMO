"""Render the inventory's scroll-bundle icon: three scrolls on a leather wrap.

    blender -b -P tools/blender-scripts/render_scroll_bundle_icon.py
"""

import math
import os
import sys

import bpy
from mathutils import Matrix

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from icon_render import add_light, principled, render_icon  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MODELS = os.path.join(REPO, "client/public/models/objects")
ICON = os.path.join(REPO, "client/public/items/objects/scroll_bundle.png")

# The scroll lies along X, about 0.08 across.
# (model, y, slide along X)
SCROLLS = [
    ("scroll.glb", -0.098, 0.015),
    ("scroll_enchant.glb", 0.0, -0.02),
    ("scroll_enchant_armor.glb", 0.098, 0.01),
]
WRAP_X = 0.40
WRAP_Y = (-0.17, 0.19)
ROLL_R = 0.038
STRAP_X = (-0.13, 0.13)


def box(size, location, material, bevel):
    bpy.ops.mesh.primitive_cube_add(size=1)
    obj = bpy.context.active_object
    obj.scale = size
    obj.location = location
    obj.data.materials.append(material)
    mod = obj.modifiers.new("bevel", "BEVEL")
    mod.width = bevel
    mod.segments = 3
    return obj


def leather_wrap():
    """An unrolled leather mat, its far end still rolled, two straps across."""
    hide = principled("hide", (0.24, 0.12, 0.05, 1.0), 0.75)
    strap = principled("strap", (0.10, 0.05, 0.025, 1.0), 0.6)
    depth = WRAP_Y[1] - WRAP_Y[0]
    middle = (WRAP_Y[0] + WRAP_Y[1]) / 2
    parts = [box((WRAP_X, depth, 0.014), (0, middle, -0.014), hide, 0.004)]
    bpy.ops.mesh.primitive_cylinder_add(radius=ROLL_R, depth=WRAP_X, vertices=48)
    roll = bpy.context.active_object
    roll.rotation_euler = (0, math.pi / 2, 0)
    roll.location = (0, WRAP_Y[1] + ROLL_R * 0.6, ROLL_R - 0.02)
    roll.data.materials.append(hide)
    for poly in roll.data.polygons:
        poly.use_smooth = len(poly.vertices) == 4
    parts.append(roll)
    for x in STRAP_X:
        parts.append(box((0.03, 0.31, 0.088), (x, 0.0, 0.03), strap, 0.006))
    return parts


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    meshes = leather_wrap()
    for model, y, slide in SCROLLS:
        before = set(bpy.context.scene.objects)
        bpy.ops.import_scene.gltf(filepath=os.path.join(MODELS, model))
        bpy.context.view_layer.update()
        place = Matrix.Translation((slide, y, 0.0))
        for obj in set(bpy.context.scene.objects) - before:
            if obj.type == "MESH":
                obj.matrix_world = place @ obj.matrix_world
                meshes.append(obj)

    add_light((0.5, -0.9, 1.4), 30.0, 1.6)
    add_light((-0.9, -0.3, 0.8), 12.0, 1.8)
    add_light((0.0, 1.0, 0.9), 8.0, 1.8)
    render_icon(meshes, ICON, (math.radians(-48), 0.0, math.radians(-28)))


main()
