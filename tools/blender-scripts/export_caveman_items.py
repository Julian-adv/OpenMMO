"""Export static inventory assets from the fitted caveman outfit."""

import argparse
from pathlib import Path
import runpy
import sys
import tempfile

import bpy

SCRIPTS = Path(__file__).resolve().parent
REPO = SCRIPTS.parents[1]
PARTS = {
    "top": ("top", "top"),
    "pants": ("pants", "pants"),
    "boots": ("boots", "boots"),
    "bracers": ("gloves", "bracer"),
}
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--parts", nargs="+", choices=PARTS, default=PARTS)
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])

with tempfile.TemporaryDirectory(prefix="caveman-items-") as temporary:
    for name in args.parts:
        part, source_part = PARTS[name]
        bpy.ops.wm.read_factory_settings(use_empty=True)
        source = REPO / f"assets/modular_human_male_01/parts/caveman_tripo_{source_part}_v1/{part}_caveman.glb"
        bpy.ops.import_scene.gltf(filepath=str(source))
        meshes = [obj for obj in bpy.context.scene.objects
                  if obj.type == "MESH" and obj.get("part_id") == f"{part}_caveman"
                  and (name != "bracers" or obj.name == "caveman_bracer_left")]
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
        static = Path(temporary) / f"caveman_{name}.glb"
        bpy.ops.export_scene.gltf(
            filepath=str(static), export_format="GLB", use_selection=True,
            export_animations=False, export_skins=False, export_extras=False,
        )
        sys.argv = [str(SCRIPTS / "export_item_asset.py"), "--",
                    "--source", str(static), "--name", f"caveman_{name}",
                    "--category", "armor", "--size", str(size),
                    "--rotation", "-90" if name in ("top", "pants") else "0", "0", "0",
                    "--icon-rotation", "25" if name in ("top", "pants") else "-65", "-8", "-12",
                    "--exposure", "-1.2"]
        bpy.context.preferences.filepaths.save_version = 0
        runpy.run_path(str(SCRIPTS / "export_item_asset.py"), run_name="__main__")
