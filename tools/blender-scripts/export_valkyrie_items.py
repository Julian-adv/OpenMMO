import argparse
import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, export_glb, render_icon

ROOT = Path(__file__).resolve().parents[2]
SOURCES = {
    "helmet": "valkyrie_helmet/fitted_v2/helmet_female_valkyrie.glb",
    "top": "valkyrie_starter/tripo_top_v1/fitted_v11/top_female_valkyrie.glb",
    "pants": "valkyrie_starter/tripo_bottom_v1/fitted_v8/bottom_female_valkyrie.glb",
    "gloves": "valkyrie_gloves/fitted_v3/bracers_female_valkyrie.glb",
    "boots": "valkyrie_boots/fitted_v1/boots_female_valkyrie.glb",
}
parser = argparse.ArgumentParser()
parser.add_argument("--icons-only", action="store_true")
parser.add_argument("--parts", nargs="+", choices=SOURCES)
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])


def arrange_pair(meshes, armature, part):
    groups = []
    for side, sign in [("Left", 1), ("Right", -1)]:
        group = []
        for obj in meshes:
            icon = obj.copy()
            icon.data = obj.data.copy()
            bpy.context.collection.objects.link(icon)
            mesh = bmesh.new()
            mesh.from_mesh(icon.data)
            bmesh.ops.delete(mesh, geom=[v for v in mesh.verts if v.co.x * sign < 0], context="VERTS")
            mesh.to_mesh(icon.data)
            mesh.free()
            if icon.data.polygons:
                group.append(icon)
            else:
                bpy.data.objects.remove(icon, do_unlink=True)
        start, end = ("ForeArm", "Hand") if part == "gloves" else ("Leg", "Foot")
        direction = armature.matrix_world.to_3x3() @ (
            armature.pose.bones[side + end].head - armature.pose.bones[side + start].head
        )
        rotation = direction.normalized().rotation_difference(Vector((0, 0, -1))).to_matrix().to_4x4()
        for obj in group:
            obj.data.transform(rotation)
        vertices = [v.co for obj in group for v in obj.data.vertices]
        minimum = Vector(tuple(min(v[i] for v in vertices) for i in range(3)))
        maximum = Vector(tuple(max(v[i] for v in vertices) for i in range(3)))
        groups.append((group, minimum, maximum))
    widths = [maximum.x - minimum.x for _, minimum, maximum in groups]
    gap = max(widths) * 0.18
    cursor = -(sum(widths) + gap) / 2
    result = []
    for (group, minimum, maximum), width in zip(groups, widths):
        center = (minimum + maximum) / 2
        translation = Vector((cursor + width / 2 - center.x, -center.y, -minimum.z))
        for obj in group:
            obj.data.transform(Matrix.Translation(translation))
        result.extend(group)
        cursor += width + gap
    for obj in meshes:
        obj.hide_render = True
    return result


for part, source in SOURCES.items():
    if args.parts and part not in args.parts:
        continue
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(ROOT / "assets/items" / source))
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH" and obj.visible_get()]
    armature = next(obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE")
    graph = bpy.context.evaluated_depsgraph_get()
    for obj in meshes:
        matrix = obj.matrix_world.copy()
        evaluated = bpy.data.meshes.new_from_object(obj.evaluated_get(graph))
        evaluated.transform(matrix)
        obj.parent = None
        obj.modifiers.clear()
        obj.data = evaluated
        obj.matrix_world = Matrix.Identity(4)
    vertices = [v.co for obj in meshes for v in obj.data.vertices]
    minimum = Vector(tuple(min(v[i] for v in vertices) for i in range(3)))
    maximum = Vector(tuple(max(v[i] for v in vertices) for i in range(3)))
    center = (minimum + maximum) / 2
    center.z = minimum.z
    for obj in meshes:
        obj.data.transform(Matrix.Translation(-center))
    model = ROOT / f"client/public/models/armor/valkyrie_{part}.glb"
    if not args.icons_only:
        export_glb(meshes, str(model))
    if part in {"gloves", "boots"}:
        meshes = arrange_pair(meshes, armature, part)
    vertices = [v.co for obj in meshes for v in obj.data.vertices]
    minimum = Vector(tuple(min(v[i] for v in vertices) for i in range(3)))
    maximum = Vector(tuple(max(v[i] for v in vertices) for i in range(3)))
    center = (minimum + maximum) / 2
    turn = Matrix.Rotation(math.radians(30), 4, "Z")
    for obj in meshes:
        obj.data.transform(turn @ Matrix.Translation(-center))
    size = max(maximum - minimum)
    for obj in meshes:
        obj.scale *= 0.5 / size
    for location, energy in [((0.7, -0.9, 1.1), 45), ((-1, -0.6, 0.4), 14), ((-0.4, 1, 0.8), 22)]:
        add_light(location, energy, 1.5)
    world = bpy.data.worlds.new("IconWorld")
    bpy.context.scene.world = world
    world.use_nodes = True
    background = world.node_tree.nodes["Background"]
    background.inputs["Color"].default_value = (0.55, 0.57, 0.62, 1)
    background.inputs["Strength"].default_value = 1.2
    bpy.context.scene.view_settings.exposure = -0.6
    icon = ROOT / f"client/public/items/armor/valkyrie_{part}.png"
    render_icon(meshes, str(icon), tuple(math.radians(a) for a in (-65, 0, -8)), margin=1.08)
