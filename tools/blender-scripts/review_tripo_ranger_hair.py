import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01'
OUTPUT = PARTS / 'hair/tripo_ranger_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_male_01/parts/ranger'
spec = importlib.util.spec_from_file_location('studio', ROOT / 'tools/blender-scripts/review_tripo_pants.py')
studio = importlib.util.module_from_spec(spec)
spec.loader.exec_module(studio)
review = studio.review


def main(directory):
    parser = argparse.ArgumentParser()
    parser.add_argument('--raw', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    bpy.ops.wm.read_factory_settings(use_empty=True)
    if args.raw:
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        meshes = [obj for obj in bpy.data.objects if obj.type == 'MESH']
        diagnostics, low, high = studio.source_review.diagnostics(meshes)
        report = dict(source=review.file_record(OUTPUT / 'source.glb'), **diagnostics)
        (ROOT / 'doc/assets/modular-ranger-tripo-hair-source-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')
        center, scale = (low + high) / 2, (high.z - low.z) * 1.15
    else:
        bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
        rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
        for obj in bpy.data.objects:
            if obj.type == 'MESH':
                obj.hide_render = obj.get('region') not in ['head', 'neck']
        review.import_part(OUTPUT / 'hair_ranger.glb', rig)
        center, scale = Vector((0, 0, 1.76)), .55
    scene, camera = studio.setup(True)
    scene.render.resolution_percentage = 100
    for name, offset in [('front', (0, -5, 0)), ('side', (5, 0, 0)), ('back', (0, 5, 0))]:
        camera.location = center + Vector(offset)
        camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(directory / f'{name}.png')
        bpy.ops.render.render(write_still=True)
    suffix = 'source' if args.raw else 'fitted'
    review.contact_sheet(directory, ['front', 'side', 'back'], ['FRONT', 'SIDE', 'BACK'], 3,
                         IMAGES / f'tripo-hair-{suffix}-v1.png')
    if not args.raw:
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        references = bpy.data.collections.new('Original Tripo hair')
        scene.collection.children.link(references)
        for obj in set(bpy.data.objects) - before:
            for collection in list(obj.users_collection):
                collection.objects.unlink(obj)
            references.objects.link(obj)
        references.hide_render = references.hide_viewport = True
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'ranger-hair-fitting.blend'))


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='ranger-hair-review-') as temporary:
        main(Path(temporary))
