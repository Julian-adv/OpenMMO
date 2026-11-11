"""Build the family banner (standing) and pennant (wall) furniture with icons.

Each model keeps its cloth as a separate mesh named `emblem_cloth`, UV-mapped
over the whole cloth at the cape print aspect (320:512), so the client lays
the family emblem on it.
"""

import json
import math
from pathlib import Path
import sys

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, export_glb, principled, render_icon

REPO = Path(__file__).resolve().parents[2]
MODELS = REPO / "client/public/models/objects"
ICONS = REPO / "client/public/items/objects"
PRINT_ASPECT = 320 / 512
ICON_CLOTH = (0.43, 0.09, 0.13, 1)


def slab(name, x, y, z, w, d, h, material):
    bpy.ops.mesh.primitive_cube_add(size=1, location=(x, y, z + h / 2))
    obj = bpy.context.object
    obj.name = name
    obj.scale = (w, d, h)
    bpy.ops.object.transform_apply(scale=True)
    obj.data.materials.append(material)
    return obj


def rod(name, x, y, z, radius, length, material, axis="Z"):
    rotation = {"Z": (0, 0, 0), "X": (0, math.pi / 2, 0)}[axis]
    bpy.ops.mesh.primitive_cylinder_add(vertices=10, radius=radius, depth=length,
                                        location=(x, y, z), rotation=rotation)
    obj = bpy.context.object
    obj.name = name
    obj.data.materials.append(material)
    return obj


def knob(name, x, y, z, radius, material):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=10, ring_count=6, radius=radius, location=(x, y, z))
    obj = bpy.context.object
    obj.name = name
    obj.data.materials.append(material)
    return obj


def cloth(width, height, top, y, tail, material):
    """A hanging sheet facing -Y with a swallowtail notch `tail` deep. UVs span
    the full rectangle, so the notch only cuts the picture's bottom edge."""
    cols, rows = 6, 10
    verts, uvs = [], []
    for r in range(rows + 1):
        v = r / rows
        for c in range(cols + 1):
            u = c / cols
            z = top - height + v * height
            if r == 0:
                z += tail * (1 - abs(u - 0.5) * 2)
            sag = 0.012 * math.sin(math.pi * u) * (1 - v)
            verts.append(((u - 0.5) * width, y + sag, z))
            uvs.append((u, v if r else (z - (top - height)) / height))
    faces = []
    for r in range(rows):
        for c in range(cols):
            a = r * (cols + 1) + c
            faces.append((a, a + 1, a + cols + 2, a + cols + 1))
    mesh = bpy.data.meshes.new("emblem_cloth")
    mesh.from_pydata(verts, [], faces)
    layer = mesh.uv_layers.new(name="UVMap")
    for poly in mesh.polygons:
        for loop in poly.loop_indices:
            layer.data[loop].uv = uvs[mesh.loops[loop].vertex_index]
    mesh.update()
    obj = bpy.data.objects.new("emblem_cloth", mesh)
    bpy.context.collection.objects.link(obj)
    obj.data.materials.append(material)
    return obj


def join(parts, name):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in parts:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.convert(target="MESH")
    bpy.ops.object.join()
    obj = bpy.context.object
    obj.name = name
    return obj


def triangles(objects):
    return sum(len(p.vertices) - 2 for obj in objects for p in obj.data.polygons)


def lights():
    for position, energy in (((1.6, -2.4, 3.0), 160), ((-2.4, -1.2, 1.6), 50), ((-0.8, 2.4, 2.4), 60)):
        add_light(position, energy, 2.5)
    scene = bpy.context.scene
    scene.world = bpy.data.worlds.new("IconWorld")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.0
    scene.view_settings.exposure = -1.0


def materials():
    wood = principled("Wood", (0.3, 0.18, 0.09, 1), 0.85)
    iron = principled("Iron", (0.08, 0.08, 0.08, 1), 0.45, 0.8)
    brass = principled("Brass", (0.75, 0.55, 0.2, 1), 0.35, 1.0)
    fabric = principled("EmblemCloth", (1, 1, 1, 1), 0.8)
    return wood, iron, brass, fabric


def build_banner():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    wood, iron, brass, fabric = materials()
    pole_h, bar_z, width = 2.5, 2.3, 0.8
    height = width / PRINT_ASPECT
    frame = join([
        slab("foot_x", 0, 0, 0, 0.62, 0.08, 0.06, wood),
        slab("foot_y", 0, 0, 0, 0.08, 0.62, 0.06, wood),
        rod("pole", 0, 0.05, pole_h / 2, 0.03, pole_h, wood),
        rod("bar", 0, 0.05, bar_z, 0.018, width + 0.12, iron, "X"),
        knob("finial", 0, 0.05, pole_h + 0.03, 0.05, brass),
        knob("bar_l", -(width + 0.12) / 2, 0.05, bar_z, 0.028, brass),
        knob("bar_r", (width + 0.12) / 2, 0.05, bar_z, 0.028, brass),
    ], "family_banner")
    sheet = cloth(width, height, bar_z - 0.02, 0.0, 0.16, fabric)
    finish("family_banner", "Family Banner", "가문 깃발", [frame, sheet],
           {"width": 0.62, "depth": 0.62, "height": pole_h + 0.08, "clothMeters": [width, round(height, 3)]},
           (math.radians(-75), 0, 0))


def build_pennant():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    wood, iron, brass, fabric = materials()
    width = 0.5
    height = width / PRINT_ASPECT
    top = height + 0.08
    frame = join([
        rod("rod", 0, 0.03, top, 0.012, width + 0.1, wood, "X"),
        knob("rod_l", -(width + 0.1) / 2, 0.03, top, 0.02, brass),
        knob("rod_r", (width + 0.1) / 2, 0.03, top, 0.02, brass),
        slab("hook", 0, 0.045, top - 0.01, 0.03, 0.03, 0.05, iron),
    ], "family_pennant")
    sheet = cloth(width, height, top - 0.012, 0.02, 0.1, fabric)
    finish("family_pennant", "Family Pennant", "가문 휘장", [frame, sheet],
           {"width": width + 0.14, "depth": 0.06, "height": round(top + 0.02, 3), "clothMeters": [width, round(height, 3)]},
           (math.radians(-75), 0, 0))


def finish(stem, name, name_ko, objects, dims, icon_rotation):
    source = REPO / "assets/items" / stem
    source.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(source / f"{stem}.blend"))
    export_glb(objects, str(MODELS / f"{stem}.glb"))
    count = triangles(objects)
    objects[1].data.materials[0] = principled("IconCloth", ICON_CLOTH, 0.8)
    lights()
    render_icon(objects, str(ICONS / f"{stem}.png"), icon_rotation, margin=1.06)
    (source / "generation.json").write_text(json.dumps({
        "name": name, "nameKo": name_ko,
        "date": "2026-11-11", "meshTool": "Blender (procedural script)",
        "blenderVersion": bpy.app.version_string,
        "script": "tools/blender-scripts/build_family_banner.py",
        "outputs": [f"client/public/models/objects/{stem}.glb", f"client/public/items/objects/{stem}.png"],
        "dimensionsMeters": dims,
        "triangles": count, "textures": 0,
        "notes": "The emblem_cloth mesh is painted with the family emblem at runtime.",
    }, ensure_ascii=False, indent=2) + "\n")


MODELS.mkdir(parents=True, exist_ok=True)
ICONS.mkdir(parents=True, exist_ok=True)
build_banner()
build_pennant()
