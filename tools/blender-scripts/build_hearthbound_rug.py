"""Build the Hearthbound Rug from its overhead artwork."""

import json
import math
from pathlib import Path
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, render_icon

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "assets/items/hearthbound_rug"
MODEL = REPO / "client/public/models/objects/hearthbound_rug.glb"
ICON = REPO / "client/public/items/objects/hearthbound_rug.png"
ART = REPO / "doc/images/props/hearthbound_rug.png"


def build():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for directory in (SOURCE, MODEL.parent, ICON.parent):
        directory.mkdir(parents=True, exist_ok=True)

    texture = bpy.data.images.load(str(ART))
    texture.scale(1024, 683)
    texture.filepath_raw = str(SOURCE / "hearthbound_rug-albedo.png")
    texture.file_format = "PNG"
    texture.save()
    texture.pack()

    material = bpy.data.materials.new("Hearthbound woven wool")
    material.use_nodes = True
    material.use_backface_culling = False
    material.surface_render_method = "DITHERED"
    nodes = material.node_tree.nodes
    links = material.node_tree.links
    shader = nodes.get("Principled BSDF")
    shader.inputs["Roughness"].default_value = 0.95
    shader.inputs["Specular IOR Level"].default_value = 0.2
    image = nodes.new("ShaderNodeTexImage")
    image.image = texture
    clip = nodes.new("ShaderNodeMath")
    clip.operation = "GREATER_THAN"
    clip.inputs[1].default_value = 0.5
    links.new(image.outputs["Color"], shader.inputs["Base Color"])
    links.new(image.outputs["Alpha"], clip.inputs[0])
    links.new(clip.outputs[0], shader.inputs["Alpha"])

    mesh = bpy.data.meshes.new("hearthbound_rug")
    mesh.from_pydata(
        [(-1.2, -0.8, 0.012), (1.2, -0.8, 0.012),
         (1.2, 0.8, 0.012), (-1.2, 0.8, 0.012)],
        [], [(0, 1, 2), (0, 2, 3)],
    )
    mesh.update()
    uv = mesh.uv_layers.new()
    coordinates = [(0, 0), (1, 0), (1, 1), (0, 1)]
    for loop in mesh.loops:
        uv.data[loop.index].uv = coordinates[loop.vertex_index]
    mesh.materials.append(material)
    rug = bpy.data.objects.new("hearthbound_rug", mesh)
    bpy.context.collection.objects.link(rug)
    bpy.context.view_layer.objects.active = rug
    rug.select_set(True)

    bpy.ops.wm.save_as_mainfile(filepath=str(SOURCE / "hearthbound_rug.blend"))
    bpy.ops.export_scene.gltf(
        filepath=str(MODEL), export_format="GLB", use_selection=True,
        export_apply=True, export_image_format="AUTO",
    )
    (SOURCE / "generation.json").write_text(json.dumps({
        "name": "Hearthbound Rug", "nameKo": "귀향의 러그",
        "date": "2026-10-03", "imageTool": "Codex built-in ImageGen",
        "tier": "ChatGPT Pro 20x", "meshTool": "Blender",
        "blenderVersion": bpy.app.version_string,
        "art": str(ART.relative_to(REPO)),
        "script": "tools/blender-scripts/build_hearthbound_rug.py",
        "dimensionsMeters": {"width": 2.4, "depth": 1.6, "surfaceHeight": 0.012},
        "triangles": 2, "alphaMode": "MASK", "alphaCutoff": 0.5,
        "textureSize": [1024, 683],
    }, ensure_ascii=False, indent=2) + "\n")

    for vertex in mesh.vertices:
        vertex.co *= 0.5 / 2.4
    rug.rotation_euler.z = math.radians(-8)
    for position, energy in (
        ((0.7, -0.9, 1.1), 45), ((-1, -0.6, 0.4), 14), ((-0.4, 1, 0.8), 22),
    ):
        add_light(position, energy, 1.5)
    scene = bpy.context.scene
    scene.world = bpy.data.worlds.new("IconWorld")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs["Strength"].default_value = 1.2
    scene.view_settings.exposure = -1.2
    render_icon([rug], str(ICON), (math.radians(25), 0, 0), margin=1.1)

    rug.parent = None
    rug.rotation_euler = (0, 0, 0)
    for vertex in mesh.vertices:
        vertex.co *= 2.4 / 0.5
    for obj in list(scene.objects):
        if obj != rug:
            bpy.data.objects.remove(obj, do_unlink=True)
    bpy.ops.mesh.primitive_cube_add(size=1, location=(0, 0, -0.06))
    floor = bpy.context.object
    floor.name = "Preview floor"
    floor.scale = (4.5, 3.5, 0.1)
    floor_material = bpy.data.materials.new("Warm floor")
    floor_material.use_nodes = True
    floor_shader = floor_material.node_tree.nodes.get("Principled BSDF")
    floor_shader.inputs["Base Color"].default_value = (0.12, 0.075, 0.04, 1)
    floor_shader.inputs["Roughness"].default_value = 0.85
    floor.data.materials.append(floor_material)
    add_light((-2, -1, 4), 450, 4)
    add_light((2, 3, 3), 200, 3)
    camera_data = bpy.data.cameras.new("Preview camera")
    camera_data.type = "ORTHO"
    camera_data.ortho_scale = 3.3
    camera = bpy.data.objects.new("Preview camera", camera_data)
    camera.location = (0.3, -2.8, 4.6)
    camera.rotation_euler = (-Vector(camera.location)).to_track_quat("-Z", "Y").to_euler()
    bpy.context.collection.objects.link(camera)
    scene.camera = camera
    scene.world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.3
    scene.view_settings.exposure = 0
    scene.render.film_transparent = False
    scene.render.resolution_x = 1200
    scene.render.resolution_y = 900
    scene.render.filepath = str(SOURCE / "hearthbound_rug-preview.png")
    bpy.ops.render.render(write_still=True)


build()
