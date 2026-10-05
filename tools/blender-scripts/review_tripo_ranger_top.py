"""Review the ranger top source or fitted candidate with sampled game poses."""
import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
SOURCE = PARTS / 'ranger_tripo_top_v4/source.glb'
IMAGES = ROOT / 'doc/images/characters/modular_human_male_01/parts/ranger'


def helper(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


review = helper('review', 'tools/blender-scripts/review_rogue_fitting.py')
raw_review = helper('raw_review', 'tools/blender-scripts/review_outfit_sources.py')


def main(directory):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--raw', action='store_true')
    parser.add_argument('--rest-only', action='store_true')
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--unmasked', action='store_true')
    parser.add_argument('--revision', type=int, default=4)
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    output = PARTS / f'ranger_tripo_top_v{args.revision}'
    fitting_report = ROOT / f'doc/assets/modular-ranger-tripo-top-fitting-v{args.revision}.json'
    animation_report = ROOT / f'doc/assets/modular-ranger-tripo-top-animation-v{args.revision}.json'
    bpy.ops.wm.read_factory_settings(use_empty=True)
    rig = None
    body = []
    hair, trousers = [], []
    if args.raw:
        bpy.ops.import_scene.gltf(filepath=str(SOURCE))
        top = [o for o in bpy.data.objects if o.type == 'MESH']
        diagnostic, low, high = raw_review.diagnostics(top)
        center = (low + high) / 2
        scale = max(high.z - low.z, (high.x - low.x) / (700 / 900)) * 1.15
        report = dict(status='Raw source only; no body fit', date='2026-10-05',
                      source=review.file_record(SOURCE), **diagnostic)
        report_path = ROOT / 'doc/assets/modular-ranger-tripo-top-source-review-v1.json'
        report_path.write_text(json.dumps(report, indent=2) + '\n')
        prefix = 'tripo-top-raw-review-v1'
    else:
        bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
        rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
        body = [o for o in bpy.data.objects if o.type == 'MESH']
        for obj in body:
            obj.hide_render = obj.get('region') in ['boot_ankles', 'legs']
            obj.hide_set(obj.hide_render)
        top = review.import_part(output / 'top_ranger.glb', rig)
        hair = review.import_part(PARTS / 'fitted/hair_crop.glb', rig)
        trousers = review.import_part(PARTS / 'fitted/pants_cloth.glb', rig)
        center, scale = Vector((0, 0, 1.40)), 1.18
        prefix = f'tripo-top-fitted-v{args.revision}'
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 12 if args.quick else 32
    scene.world = bpy.data.worlds.new('Ranger review background')
    scene.world.color = (.18, .18, .18)
    for location, energy, size in [((-3, -4, 5), 600, 4), ((4, -2, 3), 450, 4), ((0, 3, 4), 650, 3)]:
        bpy.ops.object.light_add(type='AREA', location=location)
        light = bpy.context.object
        light.data.energy, light.data.shape, light.data.size = energy, 'DISK', size
        light.rotation_euler = (center - light.location).to_track_quat('-Z', 'Y').to_euler()
    bpy.ops.object.camera_add()
    camera = bpy.context.object
    camera.data.type = 'ORTHO'
    scene.camera = camera
    scene.render.resolution_x, scene.render.resolution_y = 700, 900
    scene.render.resolution_percentage = 70 if args.quick else 100
    scene.view_settings.view_transform = 'AgX'

    def render(name, offset, target=center, ortho_scale=scale):
        camera.location = target + Vector(offset)
        camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = ortho_scale
        scene.render.filepath = str(directory / f'{name}.png')
        bpy.ops.render.render(write_still=True)

    for name, offset in [('front', (0, -5, 0)), ('side', (-5, 0, 0)), ('back', (0, 5, 0))]:
        render(name, offset)
    IMAGES.mkdir(parents=True, exist_ok=True)
    review.contact_sheet(directory, ['front', 'side', 'back'], ['FRONT', 'SIDE', 'BACK'], 3,
                         IMAGES / (prefix + ('-rest.png' if not args.raw else '.png')))
    if args.raw:
        return
    if not args.rest_only:
        animation = json.loads(animation_report.read_text())
        for source in animation['sources']:
            assert review.file_record(ROOT / source['path'])['sha256'] == source['sha256']
        snapshots = json.loads((output / 'animation-snapshots.json').read_text())
        if not args.unmasked:
            for obj in body:
                review.clip_top_skin(obj, tripo_top=True, waist_height=1.05)
                if obj.get('region') == 'upper_arms':
                    obj.hide_render = True
                    obj.hide_set(True)
        conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
        ordered = sorted(rig.pose.bones, key=lambda bone: len(bone.parent_recursive))
        for bone in rig.pose.bones:
            bone.rotation_mode = 'QUATERNION'

        def key_pose(frame):
            for bone in rig.pose.bones:
                for channel in ['location', 'rotation_quaternion', 'scale']:
                    bone.keyframe_insert(data_path=channel, frame=frame)

        key_pose(1)
        scene.timeline_markers.new('REST', frame=1)
        for index, snapshot in enumerate(snapshots, 1):
            frame = index * 10
            scene.frame_set(frame)
            for bone in ordered:
                values = snapshot['bone_deformation_matrices'].get(bone.name)
                if values is not None:
                    deformation = Matrix([values[i:i + 4] for i in range(0, 16, 4)]).transposed()
                    bone.matrix = rig.matrix_world.inverted() @ conversion @ deformation @ conversion.inverted() @ rig.matrix_world @ bone.bone.matrix_local
                    bpy.context.view_layer.update()
            key_pose(frame)
            scene.timeline_markers.new(snapshot['clip'], frame=frame)
            pose_center = rig.matrix_world @ rig.pose.bones['Hips'].head + Vector((0, 0, .38))
            render(snapshot['clip'], (-2.3, -5, .8), pose_center, 1.25)
        for layer in rig.animation_data.action.layers:
            for strip in layer.strips:
                for slot in rig.animation_data.action.slots:
                    channels = strip.channelbag(slot)
                    if channels:
                        for curve in channels.fcurves:
                            for key in curve.keyframe_points:
                                key.interpolation = 'CONSTANT'
        rig.animation_data.action.name = 'Sampled game poses'
        scene.frame_end = len(snapshots) * 10
        scene.frame_set(1)
        names = [p['clip'] for p in snapshots if p['clip'] != 'combat_idle']
        review.contact_sheet(directory, names, ['IDLE', 'WALK', 'RUN', 'JUMP', 'SLASH', 'SIT'], 3,
                             IMAGES / (prefix + '-motion.png'))
    if not args.quick:
        references = bpy.data.collections.new('Unmodified Tripo source - hidden')
        scene.collection.children.link(references)
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(SOURCE))
        for obj in set(bpy.data.objects) - before:
            for collection in list(obj.users_collection):
                collection.objects.unlink(obj)
            references.objects.link(obj)
        references.hide_render = True
        references.hide_viewport = True
        bpy.ops.object.select_all(action='DESELECT')
        top[0].select_set(True)
        bpy.context.view_layer.objects.active = top[0]
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.wm.save_as_mainfile(filepath=str(output / 'ranger-top-fitting.blend'))
        def visible_triangles(objects):
            count = 0
            for obj in objects:
                if not obj.hide_render:
                    obj.data.calc_loop_triangles()
                    count += len(obj.data.loop_triangles)
            return count
        body_triangles = visible_triangles(body)
        top_triangles = json.loads(fitting_report.read_text())['validation']['triangles']
        image_paths = [IMAGES / (prefix + '-rest.png')]
        if not args.rest_only:
            image_paths.append(IMAGES / (prefix + '-motion.png'))
        report = dict(date='2026-10-05',
                      status='Fitting candidate with sampled animations; production skin masks and mixed equipment acceptance pending',
                      blender_version=bpy.app.version_string,
                      rest_view='Torso, upper arms, neck and forearms remain visible for clearance review',
                      animation_views='Provisional review-scene skin mask; forearms and hands fully preserved',
                      scene_skin_mask=dict(hidden_regions=['boot_ankles', 'legs', 'upper_arms'],
                                           torso_maximum_y=1.05, neck_minimum_y=1.54, neck_maximum_abs_x=.075,
                                           canonical_base_glb_unchanged=True, runtime_mask_implemented=False),
                      context='Existing crop hair and cloth pants; no new ranger pants created',
                      budget=dict(top=top_triangles, base_including_hidden=13891, crop_hair=933,
                                  body_hair_top_including_hidden=13891 + 933 + top_triangles,
                                  body_hair_top_visible=body_triangles + visible_triangles(hair) + top_triangles,
                                  blender_top_triangles=visible_triangles(top),
                                  context_cloth_pants=visible_triangles(trousers), face_allocation=1505,
                                  scope='Current body, crop hair and top; final ranger pants, gloves, boots, weapon and cape not included'),
                      editable=review.file_record(output / 'ranger-top-fitting.blend'),
                      images=[review.file_record(path) for path in image_paths],
                      fitting=review.file_record(fitting_report),
                      animation=review.file_record(animation_report))
        (ROOT / f'doc/assets/modular-ranger-tripo-top-fitted-review-v{args.revision}.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='ranger-top-review-') as temporary:
        main(Path(temporary))
