"""Render a showroom model with the shared inventory icon recipe."""

import argparse
import json
import math
from pathlib import Path
import sys

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, render_icon

REPO = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("model", nargs="?", default="chair")
parser.add_argument("--geometry", type=Path)
parser.add_argument("--yaw", type=float, default=-30)
parser.add_argument("--tilt", type=float, default=-62)
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])

bpy.ops.wm.read_factory_settings(use_empty=True)
if args.geometry:
    geometry = json.loads(args.geometry.read_text())
    bpy.ops.import_scene.gltf(filepath=str(REPO / f"client/public/textures/{geometry['texture']}.glb"))
    material = next(
        material
        for obj in bpy.context.scene.objects if obj.type == "MESH"
        for material in obj.data.materials if material is not None
    )
    for obj in list(bpy.context.scene.objects):
        bpy.data.objects.remove(obj, do_unlink=True)
    positions = geometry["positions"]
    vertices = [(positions[i], -positions[i + 2], positions[i + 1]) for i in range(0, len(positions), 3)]
    indices = geometry["indices"] or list(range(len(vertices)))
    mesh = bpy.data.meshes.new(args.model)
    mesh.from_pydata(vertices, [], [indices[i:i + 3] for i in range(0, len(indices), 3)])
    mesh.update()
    uv = mesh.uv_layers.new()
    for loop in mesh.loops:
        index = loop.vertex_index * 2
        uv.data[loop.index].uv = geometry["uvs"][index:index + 2]
    mesh.materials.append(material)
    obj = bpy.data.objects.new(args.model, mesh)
    bpy.context.collection.objects.link(obj)
else:
    bpy.ops.import_scene.gltf(filepath=str(REPO / f"client/public/models/objects/{args.model}.glb"))
meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
for obj in meshes:
    obj.data.transform(obj.matrix_world)
    obj.parent = None
    obj.matrix_world = Matrix.Identity(4)
    obj.rotation_mode = "XYZ"
bpy.context.view_layer.update()
corners = [Vector(corner) for obj in meshes for corner in obj.bound_box]
lower = Vector(tuple(min(point[axis] for point in corners) for axis in range(3)))
upper = Vector(tuple(max(point[axis] for point in corners) for axis in range(3)))
center = (lower + upper) / 2
scale = 0.5 / max(upper - lower)
for obj in meshes:
    for vertex in obj.data.vertices:
        vertex.co = (vertex.co - center) * scale
    obj.rotation_euler.z = math.radians(args.yaw)

for position, energy in (
    ((0.7, -0.9, 1.1), 45),
    ((-1.0, -0.6, 0.4), 14),
    ((-0.4, 1.0, 0.8), 22),
):
    add_light(position, energy, 1.5)
scene = bpy.context.scene
scene.world = bpy.data.worlds.new("IconWorld")
scene.world.use_nodes = True
background = scene.world.node_tree.nodes["Background"]
background.inputs["Color"].default_value = (0.55, 0.57, 0.62, 1)
background.inputs["Strength"].default_value = 1.2
scene.view_settings.exposure = -1.2
render_icon(
    meshes,
    str(REPO / f"client/public/items/objects/{args.model}.png"),
    (math.radians(args.tilt), 0, 0),
    margin=1.10,
)
