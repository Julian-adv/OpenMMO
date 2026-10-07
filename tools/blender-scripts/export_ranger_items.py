"""Export static inventory assets from the fitted ranger outfit."""

import argparse
import json
from pathlib import Path
import runpy
import sys
import tempfile

import bpy
from mathutils import Matrix, Vector

SCRIPTS = Path(__file__).resolve().parent
REPO = SCRIPTS.parents[1]
SOURCES = {
    "top": "ranger_tripo_top_v4",
    "pants": "ranger_tripo_pants_v1",
    "gloves": "ranger_tripo_gloves_v1",
    "boots": "ranger_tripo_boots_v1",
}
PARTS = ("top", "pants", "boots", "gloves")
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--parts", nargs="+", choices=PARTS, default=PARTS)
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])

with tempfile.TemporaryDirectory(prefix="ranger-items-") as temporary:
    for name in args.parts:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        part_id = f"{name}_ranger"
        source = REPO / f"assets/modular_human_male_01/parts/{SOURCES[name]}/{part_id}.glb"
        bpy.ops.import_scene.gltf(filepath=str(source))
        orientation = Matrix.Identity(4)
        if name == "gloves":
            fitting = json.loads((REPO / "doc/assets/modular-ranger-tripo-gloves-fitting-v1.json").read_text())
            x, y, z = fitting["alignment"]["body_depth_axis"]
            dorsal = Vector((x, -z, y)).normalized()
            rig = next(obj for obj in bpy.context.scene.objects if obj.type == "ARMATURE")
            bones = rig.data.bones
            along = rig.matrix_world.to_3x3() @ (
                bones["RightHandMiddle1"].head_local - bones["RightHand"].head_local)
            along = (along - dorsal * along.dot(dorsal)).normalized()
            orientation = Matrix((along.cross(dorsal), along, dorsal)).to_4x4()
        meshes = [obj for obj in bpy.context.scene.objects
                  if obj.type == "MESH" and obj.get("part_id") == part_id
                  and (name != "gloves" or obj.name == "ranger_glove_right")]
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
            obj.matrix_world = orientation @ matrix
            obj.vertex_groups.clear()
            for key in list(obj.keys()):
                del obj[key]
        for obj in list(bpy.context.scene.objects):
            if obj not in meshes:
                bpy.data.objects.remove(obj, do_unlink=True)
        bpy.context.view_layer.update()
        points = [obj.matrix_world @ vertex.co for obj in meshes for vertex in obj.data.vertices]
        size = max(max(v[axis] for v in points) - min(v[axis] for v in points) for axis in range(3))
        static = Path(temporary) / f"ranger_{name}.glb"
        bpy.ops.export_scene.gltf(
            filepath=str(static), export_format="GLB", use_selection=True,
            export_animations=False, export_skins=False, export_extras=False,
        )
        icon_rotation = (0, 0, -35) if name == "gloves" else (
            25 if name in ("top", "pants") else -65, -8, -12)
        sys.argv = [str(SCRIPTS / "export_item_asset.py"), "--",
                    "--source", str(static), "--name", f"ranger_{name}",
                    "--category", "armor", "--size", str(size),
                    "--rotation", "-90" if name in ("top", "pants") else "0", "0", "0",
                    "--icon-rotation", *(str(angle) for angle in icon_rotation),
                    "--exposure", "-1.2"]
        bpy.context.preferences.filepaths.save_version = 0
        runpy.run_path(str(SCRIPTS / "export_item_asset.py"), run_name="__main__")
        (REPO / f"assets/ranger_{name}/ranger_{name}-render.png").unlink()
