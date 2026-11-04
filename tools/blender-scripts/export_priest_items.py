"""Export static inventory assets from a fitted priest or bard outfit.

Priest icons now come from export_outfit_items.py --outfit priest; rerun it after this script.
"""

import argparse
from pathlib import Path
import runpy
import sys
import tempfile

import bpy

SCRIPTS = Path(__file__).resolve().parent
REPO = SCRIPTS.parents[1]
PARTS = ("helmet", "top", "pants", "boots")
UPRIGHT_ITEMS = {"priest_boots", "bard_boots", "bard_helmet"}
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--outfit", choices=("priest", "bard"), default="priest")
parser.add_argument("--parts", nargs="+", choices=PARTS)
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
parts = args.parts or PARTS

with tempfile.TemporaryDirectory(prefix=f"{args.outfit}-items-") as temporary:
    for name in parts:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        part_id = f"{name}_{args.outfit}"
        item_name = f"{args.outfit}_{name}"
        [source] = (REPO / f"assets/modular_human_male_01/{args.outfit}").glob(f"tripo_*/{part_id}.glb")
        bpy.ops.import_scene.gltf(filepath=str(source))
        meshes = [obj for obj in bpy.context.scene.objects
                  if obj.type == "MESH" and obj.get("part_id") == part_id]
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
        static = Path(temporary) / f"{item_name}.glb"
        bpy.ops.export_scene.gltf(
            filepath=str(static), export_format="GLB", use_selection=True,
            export_animations=False, export_skins=False, export_extras=False,
        )
        lay_flat = item_name not in UPRIGHT_ITEMS
        icon_rotation = (25 if lay_flat else -65, -8, -12)
        sys.argv = [str(SCRIPTS / "export_item_asset.py"), "--",
                    "--source", str(static), "--name", item_name,
                    "--category", "armor", "--size", str(size),
                    "--rotation", "-90" if lay_flat else "0", "0", "0",
                    "--icon-rotation", *(str(angle) for angle in icon_rotation),
                    "--exposure", "-1.2"]
        bpy.context.preferences.filepaths.save_version = 0
        runpy.run_path(str(SCRIPTS / "export_item_asset.py"), run_name="__main__")
        (REPO / f"assets/items/{item_name}/{item_name}-render.png").unlink()
