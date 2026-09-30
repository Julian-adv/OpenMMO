"""Export static inventory assets from the fitted barbarian outfit."""

from pathlib import Path
import runpy
import sys
import tempfile

import bpy

SCRIPTS = Path(__file__).resolve().parent
REPO = SCRIPTS.parents[1]
PARTS = {
    "helmet": "helmet",
    "armor": "top",
    "pants": "pants",
    "boots": "boots",
    "bracers": "gloves",
}

with tempfile.TemporaryDirectory(prefix="barbarian-items-") as temporary:
    for name, part in PARTS.items():
        bpy.ops.wm.read_factory_settings(use_empty=True)
        source = REPO / f"assets/modular_human_male_01/parts/fitted/{part}_barbarian.glb"
        bpy.ops.import_scene.gltf(filepath=str(source))
        meshes = [obj for obj in bpy.context.scene.objects
                  if obj.type == "MESH" and obj.get("part_id") == f"{part}_barbarian"]
        if not meshes:
            raise ValueError(f"Missing outfit meshes: {source}")
        bpy.ops.object.select_all(action="DESELECT")
        for obj in meshes:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = meshes[0]
        bpy.ops.object.convert(target="MESH")
        for obj in meshes:
            matrix = obj.matrix_world.copy()
            obj.parent = None
            obj.matrix_world = matrix
            obj.vertex_groups.clear()
            for key in list(obj.keys()):
                del obj[key]
        for obj in list(bpy.context.scene.objects):
            if obj not in meshes:
                bpy.data.objects.remove(obj, do_unlink=True)
        bpy.context.view_layer.update()
        points = [obj.matrix_world @ vertex.co for obj in meshes for vertex in obj.data.vertices]
        size = max(max(v[axis] for v in points) - min(v[axis] for v in points) for axis in range(3))
        static = Path(temporary) / f"barbarian_{name}.glb"
        bpy.ops.export_scene.gltf(
            filepath=str(static), export_format="GLB", use_selection=True,
            export_animations=False, export_skins=False, export_extras=False,
        )
        sys.argv = [str(SCRIPTS / "export_item_asset.py"), "--",
                    "--source", str(static), "--name", f"barbarian_{name}",
                    "--category", "armor", "--size", str(size),
                    "--icon-rotation", "-65", "-8", "-12", "--exposure", "-1.2"]
        runpy.run_path(str(SCRIPTS / "export_item_asset.py"), run_name="__main__")
