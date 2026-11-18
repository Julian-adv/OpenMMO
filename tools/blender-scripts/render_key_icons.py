"""Render the six dungeon floor-key icons; `-- ring` renders the key ring instead."""

import math
import os
import sys

import bpy
from mathutils import Matrix

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from icon_render import add_light, principled, render_icon  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT_DIR = os.path.join(REPO, "client", "public", "items", "objects")

# (item id stem, base colour, roughness) per dungeon; the depth picks the teeth.
METALS = {
    "crypt_key": ((0.38, 0.36, 0.34, 1.0), 0.55),
    "orc_key": ((0.62, 0.40, 0.16, 1.0), 0.45),
    "ogre_key": ((0.10, 0.09, 0.09, 1.0), 0.40),
}
KEYS = [
    ("crypt_key", 5),
    ("orc_key", 5),
    ("orc_key", 10),
    ("ogre_key", 5),
    ("ogre_key", 10),
    ("ogre_key", 15),
]

SHAFT_LEN = 0.20
SHAFT_R = 0.014
BOW_R = 0.045
BOW_TUBE = 0.011
TOOTH = (0.022, 0.012, 0.030)
KEY_MIDDLE = (0.05, 0, 0)


def add(obj):
    bpy.context.collection.objects.link(obj)
    return obj


def build(depth, material):
    """Key lying in the XY plane: bow at -X, shaft along +X, teeth toward -Y."""
    parts = []
    bpy.ops.mesh.primitive_torus_add(
        major_radius=BOW_R, minor_radius=BOW_TUBE, major_segments=48, minor_segments=16
    )
    bow = bpy.context.active_object
    bow.location = (-BOW_R, 0, 0)
    parts.append(bow)

    bpy.ops.mesh.primitive_cylinder_add(radius=SHAFT_R, depth=SHAFT_LEN, vertices=32)
    shaft = bpy.context.active_object
    shaft.rotation_euler = (0, math.pi / 2, 0)
    shaft.location = (SHAFT_LEN / 2, 0, 0)
    parts.append(shaft)

    # Collar where the bow meets the shaft.
    bpy.ops.mesh.primitive_cylinder_add(radius=SHAFT_R * 1.6, depth=0.02, vertices=32)
    collar = bpy.context.active_object
    collar.rotation_euler = (0, math.pi / 2, 0)
    collar.location = (0.02, 0, 0)
    parts.append(collar)

    teeth = depth // 5
    for i in range(teeth):
        bpy.ops.mesh.primitive_cube_add(size=1)
        tooth = bpy.context.active_object
        tooth.scale = TOOTH
        x = SHAFT_LEN - TOOTH[0] / 2 - i * (TOOTH[0] + 0.008)
        tooth.location = (x, -TOOTH[1] / 2 - SHAFT_R * 0.6, 0)
        parts.append(tooth)

    for obj in parts:
        obj.data.materials.append(material)
        for poly in obj.data.polygons:
            poly.use_smooth = obj is not tooth if teeth else True
    return parts


def render_key(stem, depth):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    color, roughness = METALS[stem]
    parts = build(depth, principled(stem, color, roughness, metallic=0.85))
    add_light((0.6, -0.5, 1.2), 60, 1.2, aim=KEY_MIDDLE)
    add_light((-0.6, 0.4, 0.7), 25, 1.4, aim=KEY_MIDDLE)
    neutral_world()
    # Diagonal in frame, bow top-left, a slight tip so the teeth read.
    render_icon(
        parts,
        os.path.join(OUT_DIR, f"{stem}_{depth}.png"),
        (math.radians(-20), 0, math.radians(-40)),
        margin=1.08,
    )


def neutral_world():
    """Metal wants something to reflect."""
    world = bpy.data.worlds.new("world")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs[0].default_value = (0.55, 0.57, 0.62, 1.0)
    bg.inputs[1].default_value = 1.2
    bpy.context.scene.world = world


RING_R = 0.085
RING_TUBE = 0.011
# (metal, depth, hang angle in degrees, lift off the plane)
RING_KEYS = [
    ("crypt_key", 5, -128, 0.0),
    ("orc_key", 10, -90, 0.03),
    ("ogre_key", 15, -52, 0.06),
]


def render_key_ring():
    """The inventory's key-bundle cell: three keys fanned on one ring."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.mesh.primitive_torus_add(
        major_radius=RING_R, minor_radius=RING_TUBE, major_segments=64, minor_segments=16
    )
    ring = bpy.context.active_object
    ring.location = (0, 0, 0.03)
    ring.data.materials.append(
        principled("ring", (0.42, 0.40, 0.38, 1.0), 0.45, metallic=0.9)
    )
    for poly in ring.data.polygons:
        poly.use_smooth = True
    parts = [ring]
    for stem, depth, degrees, lift in RING_KEYS:
        color, roughness = METALS[stem]
        key = build(depth, principled(stem, color, roughness, metallic=0.85))
        angle = math.radians(degrees)
        reach = RING_R + BOW_R * 1.55
        hang = Matrix.Translation(
            (reach * math.cos(angle), reach * math.sin(angle), lift)
        ) @ Matrix.Rotation(angle, 4, "Z")
        bpy.context.view_layer.update()
        for obj in key:
            obj.matrix_world = hang @ obj.matrix_world
        parts.extend(key)
    middle = (0, -0.12, 0)
    add_light((0.6, -0.5, 1.2), 70, 1.2, aim=middle)
    add_light((-0.6, 0.4, 0.7), 30, 1.4, aim=middle)
    neutral_world()
    render_icon(
        parts,
        os.path.join(OUT_DIR, "key_ring.png"),
        (math.radians(-15), 0, math.radians(12)),
        margin=1.06,
    )


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if "ring" in args:
        render_key_ring()
        return
    for stem, depth in KEYS:
        render_key(stem, depth)


main()
