import math
import sys
from pathlib import Path

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

for part, source in SOURCES.items():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(ROOT / "assets/items" / source))
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH" and obj.visible_get()]
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
    export_glb(meshes, str(model))
    size = max(maximum - minimum)
    for obj in meshes:
        obj.scale *= 0.5 / size
    for location, energy in [((0.7, -0.9, 1.1), 45), ((-1, -0.6, 0.4), 14), ((-0.4, 1, 0.8), 22)]:
        add_light(location, energy, 1.5)
    world = bpy.data.worlds.new("IconWorld")
    bpy.context.scene.world = world
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.2
    icon = ROOT / f"client/public/items/armor/valkyrie_{part}.png"
    render_icon(meshes, str(icon), tuple(math.radians(a) for a in (78, -8, -12)), margin=1.10)
