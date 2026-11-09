"""Build the Notice Paper: a stamped sheet for the village notice board."""

import json
import math
from pathlib import Path
import sys

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, export_glb, principled, render_icon

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "assets/items/notice_paper"
MODEL = REPO / "client/public/models/objects/notice_paper.glb"
ICON = REPO / "client/public/items/objects/notice_paper.png"

W, D = 0.34, 0.44
THICK = 0.003


def slab(name, x, y, w, d, z, h, material):
    bpy.ops.mesh.primitive_cube_add(size=1, location=(x, y, z + h / 2))
    obj = bpy.context.object
    obj.name = name
    obj.scale = (w, d, h)
    bpy.ops.object.transform_apply(scale=True)
    obj.data.materials.append(material)
    return obj


def sheet(material):
    """Paper with a slight lift at the bottom-right corner."""
    cols, rows = 6, 8
    verts, faces = [], []
    for j in range(rows + 1):
        for i in range(cols + 1):
            u, v = i / cols, j / rows
            lift = 0.03 * max(0.0, u - 0.6) * max(0.0, 0.4 - v) / 0.16
            verts.append(((u - 0.5) * W, (v - 0.5) * D, THICK + lift))
    for j in range(rows):
        for i in range(cols):
            a = j * (cols + 1) + i
            faces.append((a, a + 1, a + cols + 2, a + cols + 1))
    mesh = bpy.data.meshes.new("notice_paper")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new("notice_paper", mesh)
    bpy.context.collection.objects.link(obj)
    mod = obj.modifiers.new("thickness", "SOLIDIFY")
    mod.thickness = THICK
    mod.offset = -1
    obj.data.materials.append(material)
    return obj


def build():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for directory in (SOURCE, MODEL.parent, ICON.parent):
        directory.mkdir(parents=True, exist_ok=True)

    paper = principled("Paper", (0.86, 0.78, 0.6, 1), 0.92)
    ink = principled("Ink", (0.12, 0.09, 0.07, 1), 0.8)
    stamp = principled("Stamp", (0.55, 0.08, 0.06, 1), 0.6)
    stamp_face = principled("Stamp face", (0.83, 0.62, 0.36, 1), 0.7)

    parts = [sheet(paper)]
    top = THICK + 0.0005
    sx, sy = W / 2 - 0.06, D / 2 - 0.065
    parts.append(slab("stamp", sx, sy, 0.075, 0.085, top, 0.0015, stamp))
    parts.append(slab("stamp_face", sx, sy, 0.055, 0.065, top + 0.0015, 0.0008, stamp_face))
    parts.append(slab("title", -0.04, D / 2 - 0.07, 0.17, 0.022, top, 0.0008, ink))
    for k, width in enumerate((0.26, 0.24, 0.26, 0.2, 0.25, 0.14)):
        y = D / 2 - 0.15 - k * 0.042
        parts.append(slab(f"line{k}", -W / 2 + 0.035 + width / 2, y, width, 0.01, top, 0.0008, ink))

    bpy.ops.object.select_all(action="DESELECT")
    for obj in parts:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.convert(target="MESH")
    bpy.ops.object.join()
    item = bpy.context.object
    item.name = "notice_paper"

    bpy.ops.wm.save_as_mainfile(filepath=str(SOURCE / "notice_paper.blend"))
    export_glb([item], str(MODEL))
    triangles = sum(len(p.vertices) - 2 for p in item.data.polygons)

    for position, energy in (((0.6, -0.8, 1.2), 40), ((-1, -0.5, 0.5), 12), ((-0.3, 1, 0.9), 18)):
        add_light(position, energy, 1.5)
    scene = bpy.context.scene
    scene.world = bpy.data.worlds.new("IconWorld")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.0
    scene.view_settings.exposure = -1.0
    render_icon([item], str(ICON), (math.radians(35), 0, math.radians(-20)), margin=1.08)

    (SOURCE / "generation.json").write_text(json.dumps({
        "name": "Notice Paper", "nameKo": "게시 용지",
        "date": "2026-11-09", "meshTool": "Blender (procedural script)",
        "blenderVersion": bpy.app.version_string,
        "script": "tools/blender-scripts/build_notice_paper.py",
        "dimensionsMeters": {"width": W, "depth": D},
        "triangles": triangles, "textures": 0,
    }, ensure_ascii=False, indent=2) + "\n")


build()
