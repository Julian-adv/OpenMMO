"""Build the mailbox furniture and the Letter item, with their icons."""

import json
import math
from pathlib import Path
import sys

import bmesh
import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, export_glb, principled, render_icon

REPO = Path(__file__).resolve().parents[2]
MODELS = REPO / "client/public/models/objects"
ICONS = REPO / "client/public/items/objects"

POST_H = 1.0
BOX_W, BOX_D, BOX_H = 0.42, 0.34, 0.26
ENV_W, ENV_D, ENV_T = 0.22, 0.13, 0.004


def slab(name, x, y, z, w, d, h, material):
    bpy.ops.mesh.primitive_cube_add(size=1, location=(x, y, z + h / 2))
    obj = bpy.context.object
    obj.name = name
    obj.scale = (w, d, h)
    bpy.ops.object.transform_apply(scale=True)
    obj.data.materials.append(material)
    return obj


def roof(z, material):
    """Half cylinder along X over the box."""
    steps, r = 10, BOX_D / 2
    arc = [(r * math.cos(math.pi * i / steps), r * math.sin(math.pi * i / steps)) for i in range(steps + 1)]
    verts = [(sx * BOX_W / 2, y, z + h) for sx in (-1, 1) for y, h in arc]
    n = steps + 1
    faces = [(i, i + 1, n + i + 1, n + i) for i in range(steps)]
    faces.append(tuple(range(n))[::-1])
    faces.append(tuple(range(n, 2 * n)))
    mesh = bpy.data.meshes.new("roof")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new("roof", mesh)
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


def triangles(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def lights():
    for position, energy in (((0.8, -1.2, 1.6), 60), ((-1.2, -0.6, 0.8), 18), ((-0.4, 1.2, 1.2), 25)):
        add_light(position, energy, 1.5)
    scene = bpy.context.scene
    scene.world = bpy.data.worlds.new("IconWorld")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.0
    scene.view_settings.exposure = -1.0


def build_mailbox():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    wood = principled("Wood", (0.32, 0.2, 0.11, 1), 0.85)
    paint = principled("Paint", (0.48, 0.07, 0.05, 1), 0.55)
    iron = principled("Iron", (0.08, 0.08, 0.08, 1), 0.45, 0.8)
    brass = principled("Brass", (0.75, 0.55, 0.2, 1), 0.35, 1.0)

    parts = [
        slab("base", 0, 0, 0, 0.26, 0.26, 0.06, wood),
        slab("post", 0, 0, 0.06, 0.1, 0.1, POST_H - 0.06, wood),
        slab("shelf", 0, 0, POST_H - 0.03, BOX_W + 0.04, BOX_D + 0.04, 0.03, wood),
        slab("box", 0, 0, POST_H, BOX_W, BOX_D, BOX_H, paint),
        roof(POST_H + BOX_H, paint),
        slab("slot", 0, -BOX_D / 2 - 0.004, POST_H + BOX_H - 0.08, 0.24, 0.012, 0.035, iron),
        slab("plate", 0, -BOX_D / 2 - 0.003, POST_H + 0.05, 0.14, 0.008, 0.07, brass),
        slab("band", 0, 0, POST_H + BOX_H - 0.01, BOX_W + 0.006, BOX_D + 0.006, 0.02, iron),
        slab("flag_arm", BOX_W / 2 + 0.012, 0.04, POST_H + 0.1, 0.02, 0.03, 0.22, iron),
        slab("flag", BOX_W / 2 + 0.012, 0.09, POST_H + 0.24, 0.016, 0.1, 0.07, paint),
    ]
    mailbox = join(parts, "mailbox")
    source = REPO / "assets/items/mailbox"
    if not ICON_ONLY:
        source.mkdir(parents=True, exist_ok=True)
        bpy.ops.wm.save_as_mainfile(filepath=str(source / "mailbox.blend"))
        export_glb([mailbox], str(MODELS / "mailbox.glb"))
    count = triangles(mailbox)
    lights()
    render_icon([box_only(mailbox)], str(ICONS / "mailbox.png"), (math.radians(-75), 0, 0), margin=1.08)
    if not ICON_ONLY:
        record(source, "Mailbox", "우체통", "mailbox", count,
               {"width": BOX_W, "depth": BOX_D, "height": round(POST_H + BOX_H + BOX_D / 2, 3)})


def box_only(mailbox):
    """Icon subject: the box upright with the post cut off, turned to show the flag side."""
    icon = mailbox.copy()
    icon.data = mailbox.data.copy()
    bpy.context.collection.objects.link(icon)
    mailbox.hide_render = True
    bm = bmesh.new()
    bm.from_mesh(icon.data)
    geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
    bmesh.ops.bisect_plane(bm, geom=geom, plane_co=(0, 0, POST_H - 0.3), plane_no=(0, 0, 1),
                           clear_inner=True)
    bm.to_mesh(icon.data)
    bm.free()
    icon.rotation_euler.z = math.radians(-30)
    bpy.context.view_layer.objects.active = icon
    bpy.ops.object.select_all(action="DESELECT")
    icon.select_set(True)
    bpy.ops.object.transform_apply(rotation=True)
    return icon


def build_letter():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    paper = principled("Envelope", (0.88, 0.8, 0.64, 1), 0.9)
    fold = principled("Fold", (0.7, 0.6, 0.45, 1), 0.9)
    wax = principled("Wax", (0.5, 0.05, 0.04, 1), 0.35)

    top = ENV_T + 0.0004
    parts = [slab("envelope", 0, 0, 0, ENV_W, ENV_D, ENV_T, paper)]
    for side in (-1, 1):
        crease = slab(f"crease{side}", side * ENV_W / 4, ENV_D / 4 - 0.02, top - 0.0004,
                      ENV_W / 2 * 1.08, 0.004, 0.0008, fold)
        crease.rotation_euler.z = side * math.radians(28)
        parts.append(crease)
    bpy.ops.mesh.primitive_cylinder_add(vertices=14, radius=0.022, depth=0.004,
                                        location=(0, -0.005, top + 0.002))
    seal = bpy.context.object
    seal.data.materials.append(wax)
    parts.append(seal)
    letter = join(parts, "letter")
    source = REPO / "assets/items/letter"
    source.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(source / "letter.blend"))
    export_glb([letter], str(MODELS / "letter.glb"))
    count = triangles(letter)
    lights()
    render_icon([letter], str(ICONS / "letter.png"), (math.radians(35), 0, math.radians(-20)), margin=1.08)
    record(source, "Letter", "편지지", "letter", count, {"width": ENV_W, "depth": ENV_D})


def record(source, name, name_ko, stem, count, dims):
    (source / "generation.json").write_text(json.dumps({
        "name": name, "nameKo": name_ko,
        "date": "2026-11-09", "meshTool": "Blender (procedural script)",
        "blenderVersion": bpy.app.version_string,
        "script": "tools/blender-scripts/build_mailbox.py",
        "outputs": [f"client/public/models/objects/{stem}.glb", f"client/public/items/objects/{stem}.png"],
        "dimensionsMeters": dims,
        "triangles": count, "textures": 0,
    }, ensure_ascii=False, indent=2) + "\n")


ICON_ONLY = "--icon-only" in sys.argv
MODELS.mkdir(parents=True, exist_ok=True)
ICONS.mkdir(parents=True, exist_ok=True)
build_mailbox()
if not ICON_ONLY:
    build_letter()
