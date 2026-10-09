"""Render ranger trousers with the current top and sampled game poses."""
import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01'
OUTPUT = PARTS / 'ranger/tripo_pants_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_male_01/parts/ranger'


def helper(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


review = helper('review', 'tools/blender-scripts/review_rogue_fitting.py')
raw_review = helper('raw_review', 'tools/blender-scripts/review_outfit_sources.py')
trouser_review = helper('trouser_review', 'tools/blender-scripts/review_tripo_pants.py')


def main(directory):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--raw', action='store_true')
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--rest-only', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    bpy.ops.wm.read_factory_settings(use_empty=True)
    body, appearance = [], []
    if args.raw:
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        pants = [o for o in bpy.data.objects if o.type == 'MESH']
        diagnostic, low, high = raw_review.diagnostics(pants)
        center, scale = (low + high) / 2, (high.z - low.z) * 1.18
        report = dict(status='Raw source; not fitted or rigged',
                      source=review.file_record(OUTPUT / 'source.glb'), **diagnostic)
        (ROOT / 'doc/assets/modular-ranger-tripo-pants-source-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')
        prefix = 'tripo-pants-source-v1'
    else:
        bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
        rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
        body = [o for o in bpy.data.objects if o.type == 'MESH']
        pants = review.import_part(OUTPUT / 'pants_ranger.glb', rig)
        appearance = body + pants
        for path in ['ranger/tripo_top_v4/top_ranger.glb', 'fitted/hair_crop.glb']:
            appearance.extend(review.import_part(PARTS / path, rig))
        for obj in body:
            review.clip_top_skin(obj, True, waist_height=1.05)
            obj.hide_render = obj.get('region') in ['torso', 'upper_arms', 'legs', 'ankles', 'boot_ankles']
            obj.hide_set(obj.hide_render)
        center, scale = Vector((0, 0, .65)), 1.45
        prefix = 'tripo-pants-fitted-v1'
    scene, camera = trouser_review.setup(args.quick)

    def render(name, offset, target=center, ortho_scale=scale):
        camera.location = target + Vector(offset)
        camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = ortho_scale
        scene.render.filepath = str(directory / f'{name}.png')
        bpy.ops.render.render(write_still=True)

    for name, offset in [('front', (0, -5, 0)), ('back', (0, 5, 0)), ('side', (-5, 0, 0))]:
        render(name, offset)
    review.contact_sheet(directory, ['front', 'back', 'side'], ['FRONT', 'BACK', 'SIDE'], 3,
                         IMAGES / (prefix + '-rest.png'))
    if args.raw:
        return
    render('hero', (-2.5, -5, .4), Vector((0, 0, 1)), 2.13)
    render('waist', (0, -5, 0), Vector((0, 0, 1.075)), .62)
    render('waist-back', (0, 5, 0), Vector((0, 0, 1.075)), .62)
    review.contact_sheet(directory, ['hero', 'waist', 'waist-back'], ['FULL SET', 'WAIST FRONT', 'WAIST BACK'], 3,
                         IMAGES / (prefix + '-connections.png'))
    if not args.rest_only:
        conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
        ordered = sorted(rig.pose.bones, key=lambda bone: len(bone.parent_recursive))
        for bone in rig.pose.bones:
            bone.rotation_mode = 'QUATERNION'
        snapshots = json.loads((OUTPUT / 'animation-snapshots.json').read_text())
        for bone in rig.pose.bones:
            for channel in ['location', 'rotation_quaternion', 'scale']:
                bone.keyframe_insert(data_path=channel, frame=1)
        scene.timeline_markers.new('REST', frame=1)
        for index, snapshot in enumerate(snapshots, 1):
            frame = index * 10
            scene.frame_set(frame)
            for bone in ordered:
                values = snapshot['bone_deformation_matrices'].get(bone.name)
                deformation = Matrix([values[i:i + 4] for i in range(0, 16, 4)]).transposed()
                bone.matrix = rig.matrix_world.inverted() @ conversion @ deformation @ conversion.inverted() @ rig.matrix_world @ bone.bone.matrix_local
                bpy.context.view_layer.update()
            for bone in rig.pose.bones:
                for channel in ['location', 'rotation_quaternion', 'scale']:
                    bone.keyframe_insert(data_path=channel, frame=frame)
            scene.timeline_markers.new(snapshot['clip'], frame=frame)
            depsgraph = bpy.context.evaluated_depsgraph_get()
            bounds = []
            for obj in appearance:
                if obj.hide_render:
                    continue
                evaluated = obj.evaluated_get(depsgraph)
                mesh = evaluated.to_mesh()
                bounds.extend(evaluated.matrix_world @ v.co for v in mesh.vertices)
                evaluated.to_mesh_clear()
            pose_center = Vector(tuple((min(p[i] for p in bounds) + max(p[i] for p in bounds)) / 2 for i in range(3)))
            rotation = Vector((2.3, 5, -.9)).to_track_quat('-Z', 'Y').to_matrix().transposed()
            projected = [rotation @ (point - pose_center) for point in bounds]
            width = 2 * max(abs(point.x) for point in projected)
            height = 2 * max(abs(point.y) for point in projected)
            pose_scale = max(height * 1.15, width * 900 / 700 * 1.15)
            render(snapshot['clip'], (-2.3, -5, .9), pose_center, pose_scale)
            if snapshot['clip'] in ['run', 'sit_idle']:
                render(snapshot['clip'] + '-back', (0, 5, .9), pose_center, pose_scale)
        names = [p['clip'] for p in snapshots if p['clip'] != 'combat_idle']
        review.contact_sheet(directory, names, ['IDLE', 'WALK', 'RUN', 'JUMP', 'SLASH', 'SIT'], 3,
                             IMAGES / (prefix + '-motion.png'))
        review.contact_sheet(directory, ['run', 'run-back', 'sit_idle', 'sit_idle-back'],
                             ['RUN FRONT', 'RUN BACK', 'SIT FRONT', 'SIT BACK'], 2,
                             IMAGES / (prefix + '-rear-motion.png'))
        scene.frame_set(1)
    if args.quick:
        return
    references = bpy.data.collections.new('Unmodified source and canonical interfaces - hidden')
    scene.collection.children.link(references)
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
    for obj in set(bpy.data.objects) - before:
        for collection in list(obj.users_collection):
            collection.objects.unlink(obj)
        references.objects.link(obj)
    with bpy.data.libraries.load(str(PARTS / 'interfaces/v1/outfit-reference.blend'), link=False) as (source, target):
        target.objects = [n for n in source.objects if n.endswith(('_center', '_band_minus', '_band_plus'))]
    for obj in target.objects:
        if obj:
            references.objects.link(obj)
    references.hide_render = references.hide_viewport = True
    bpy.ops.object.select_all(action='DESELECT')
    pants[0].select_set(True)
    bpy.context.view_layer.objects.active = pants[0]
    bpy.ops.file.pack_all()
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'ranger-pants-fitting.blend'))
    report = dict(status='Dev fitting candidate; sampled game poses reviewed, other mixed outfits pending',
                  editable=review.file_record(OUTPUT / 'ranger-pants-fitting.blend'),
                  images=[review.file_record(IMAGES / (prefix + suffix)) for suffix in ['-rest.png', '-connections.png', '-motion.png', '-rear-motion.png']],
                  context=['canonical base', 'ranger top v4', 'crop hair', 'bare feet'],
                  hidden_regions=['torso', 'upper_arms', 'legs', 'ankles', 'boot_ankles'])
    (ROOT / 'doc/assets/modular-ranger-tripo-pants-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='ranger-pants-review-') as directory:
        main(Path(directory))
