"""Build the Revenue Stamp: the land registrar's fee stamp for family filings."""

import json
import math
from pathlib import Path
import sys

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, export_glb, principled, render_icon

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "assets/items/revenue_stamp"
MODEL = REPO / "client/public/models/objects/revenue_stamp.glb"
ICON = REPO / "client/public/items/objects/revenue_stamp.png"

W, D = 0.2, 0.26
THICK = 0.003
TEETH_W, TEETH_D = 8, 10
BITE = 0.008


def polygon(name, points, z, h, material):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([(x, y, z) for x, y in points], [], [tuple(range(len(points)))])
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    mod = obj.modifiers.new("thickness", "SOLIDIFY")
    mod.thickness = h
    mod.offset = 1
    obj.data.materials.append(material)
    return obj


def rect(w, d, cx=0.0, cy=0.0):
    return [(cx - w / 2, cy - d / 2), (cx + w / 2, cy - d / 2),
            (cx + w / 2, cy + d / 2), (cx - w / 2, cy + d / 2)]


def perforated():
    """Counter-clockwise outline with square perforation notches on every edge."""
    def edge(start, end, teeth, inward):
        points = []
        for k in range(teeth * 2):
            a = k / (teeth * 2)
            b = (k + 1) / (teeth * 2)
            depth = BITE if k % 2 else 0.0
            for t in (a, b):
                x = start[0] + (end[0] - start[0]) * t + inward[0] * depth
                y = start[1] + (end[1] - start[1]) * t + inward[1] * depth
                points.append((x, y))
        return points

    hw, hd = W / 2, D / 2
    outline = []
    outline += edge((-hw, -hd), (hw, -hd), TEETH_W, (0, 1))
    outline += edge((hw, -hd), (hw, hd), TEETH_D, (-1, 0))
    outline += edge((hw, hd), (-hw, hd), TEETH_W, (0, -1))
    outline += edge((-hw, hd), (-hw, -hd), TEETH_D, (1, 0))
    deduped = [outline[0]]
    for point in outline[1:]:
        if math.dist(point, deduped[-1]) > 1e-6:
            deduped.append(point)
    if math.dist(deduped[0], deduped[-1]) < 1e-6:
        deduped.pop()
    return deduped


def build():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for directory in (SOURCE, MODEL.parent, ICON.parent):
        directory.mkdir(parents=True, exist_ok=True)

    paper = principled("Paper", (0.88, 0.82, 0.66, 1), 0.92)
    red = principled("Stamp red", (0.55, 0.08, 0.06, 1), 0.6)
    face = principled("Stamp face", (0.86, 0.7, 0.46, 1), 0.7)
    ink = principled("Ink", (0.2, 0.05, 0.04, 1), 0.8)

    top = THICK
    parts = [polygon("revenue_stamp", perforated(), 0.0, THICK, paper)]
    parts.append(polygon("field", rect(W - 0.045, D - 0.045), top, 0.0012, red))
    parts.append(polygon("face", rect(W - 0.075, D - 0.075), top + 0.0012, 0.0008, face))
    crest_z = top + 0.002
    crest = [(-0.03, 0.06), (0.03, 0.06), (0.03, 0.015), (0.0, -0.02), (-0.03, 0.015)]
    parts.append(polygon("crest", crest, crest_z, 0.0008, red))
    inner = [(-0.016, 0.047), (0.016, 0.047), (0.016, 0.02), (0.0, 0.0), (-0.016, 0.02)]
    parts.append(polygon("crest_inner", inner, crest_z + 0.0008, 0.0006, face))
    for k, width in enumerate((0.085, 0.06)):
        parts.append(polygon(f"line{k}", rect(width, 0.009, 0.0, -0.045 - k * 0.02),
                             crest_z, 0.0008, ink))

    bpy.ops.object.select_all(action="DESELECT")
    for obj in parts:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.convert(target="MESH")
    bpy.ops.object.join()
    item = bpy.context.object
    item.name = "revenue_stamp"
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.quads_convert_to_tris(quad_method="BEAUTY", ngon_method="BEAUTY")
    bpy.ops.mesh.normals_make_consistent(inside=False)
    bpy.ops.object.mode_set(mode="OBJECT")

    bpy.ops.wm.save_as_mainfile(filepath=str(SOURCE / "revenue_stamp.blend"))
    export_glb([item], str(MODEL))
    triangles = len(item.data.polygons)

    for position, energy in (((0.6, -0.8, 1.2), 40), ((-1, -0.5, 0.5), 12), ((-0.3, 1, 0.9), 18)):
        add_light(position, energy, 1.5)
    scene = bpy.context.scene
    scene.world = bpy.data.worlds.new("IconWorld")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.0
    scene.view_settings.exposure = -1.0
    render_icon([item], str(ICON), (math.radians(35), 0, math.radians(-20)), margin=1.08)

    (SOURCE / "generation.json").write_text(json.dumps({
        "name": "Revenue Stamp", "nameKo": "인지",
        "date": "2026-11-11", "meshTool": "Blender (procedural script)",
        "blenderVersion": bpy.app.version_string,
        "script": "tools/blender-scripts/build_revenue_stamp.py",
        "dimensionsMeters": {"width": W, "depth": D},
        "triangles": triangles, "textures": 0,
    }, ensure_ascii=False, indent=2) + "\n")


build()
