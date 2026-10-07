"""Review ranger boots and preserve the editable fitting scene."""
import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
OUTPUT = PARTS / 'ranger_tripo_boots_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_male_01/parts/ranger'


def helper(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


review = helper('review', 'tools/blender-scripts/review_rogue_fitting.py')
raw_review = helper('raw_review', 'tools/blender-scripts/review_outfit_sources.py')
studio = helper('studio', 'tools/blender-scripts/review_tripo_pants.py')


def main(directory):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--raw', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    bpy.ops.wm.read_factory_settings(use_empty=True)
    if args.raw:
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        meshes = [o for o in bpy.data.objects if o.type == 'MESH']
        diagnostic, low, high = raw_review.diagnostics(meshes)
        report = dict(source=review.file_record(OUTPUT / 'source.glb'),
                      status='Raw single boot; not fitted or rigged', **diagnostic)
        (ROOT / 'doc/assets/modular-ranger-tripo-boots-source-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')
        center, scale = (low + high) / 2, (high.z - low.z) * 1.15
        prefix = 'tripo-boots-source-v1'
    else:
        bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
        rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
        body = [o for o in bpy.data.objects if o.type == 'MESH']
        review.import_part(OUTPUT / 'boots_ranger.glb', rig)
        for part in ['ranger_tripo_top_v4/top_ranger.glb', 'ranger_tripo_boots_v1/pants_ranger_boots-review.glb', 'fitted/hair_crop.glb']:
            review.import_part(PARTS / part, rig)
        for obj in body:
            obj.hide_render = obj.get('region') in ['torso', 'upper_arms', 'legs', 'ankles', 'boot_ankles', 'feet']
            obj.hide_set(obj.hide_render)
            if obj.get('region') == 'neck':
                review.clip_top_skin(obj, True)
        center, scale = Vector((0, 0, .28)), .68
        prefix = 'tripo-boots-fitted-v1'
    scene, camera = studio.setup(True)
    for name, offset in [('front', (0, -5, 0)), ('back', (0, 5, 0)), ('side', (5, 0, 0))]:
        camera.location = center + Vector(offset)
        camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(directory / f'{name}.png')
        bpy.ops.render.render(write_still=True)
    review.contact_sheet(directory, ['front', 'back', 'side'], ['FRONT', 'BACK', 'SIDE'], 3, IMAGES / f'{prefix}-rest.png')
    if not args.raw:
        guides = bpy.data.collections.new('Interfaces v1')
        scene.collection.children.link(guides)
        guides.hide_render = True
        data = json.loads((PARTS / 'interfaces/v1/interfaces.json').read_text())
        for entry in data['interfaces']:
            if not entry['name'].startswith(('boot_calf_', 'shoe_ankle_')):
                continue
            for name, contour in entry['contours'].items():
                curve = bpy.data.curves.new(entry['name'] + '_' + name, 'CURVE')
                curve.dimensions = '3D'
                spline = curve.splines.new('POLY')
                spline.points.add(len(contour['points']) - 1)
                for point, (x, y, z) in zip(spline.points, contour['points']):
                    point.co = (x, -z, y, 1)
                spline.use_cyclic_u = True
                guides.objects.link(bpy.data.objects.new(curve.name, curve))
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        source = bpy.data.collections.new('Original unmodified Tripo boot')
        scene.collection.children.link(source)
        for obj in set(bpy.data.objects) - before:
            for collection in list(obj.users_collection):
                collection.objects.unlink(obj)
            source.objects.link(obj)
        source.hide_render = source.hide_viewport = True
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'ranger-boots-fitting.blend'))


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='ranger-boots-review-') as directory:
        main(Path(directory))
