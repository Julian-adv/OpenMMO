"""Review the caveman top with the canonical body and sampled game poses."""
import argparse
import importlib.util
import json
import shutil
import sys
import tempfile
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
OUTPUT = PARTS / 'caveman_tripo_top_v1'
PREFIX = ROOT / 'doc/images/characters/modular_human_male_01/parts/caveman/tripo-top-fitted-v1'
spec = importlib.util.spec_from_file_location('review', ROOT / 'tools/blender-scripts/review_rogue_fitting.py')
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)


def main(images):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--rest-only', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
    rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
    for obj in bpy.data.objects:
        if obj.type == 'MESH':
            obj.hide_render = obj.get('region') == 'boot_ankles'
            obj.hide_set(obj.hide_render)
    top = [obj for obj in review.import_part(OUTPUT / 'top_caveman.glb', rig) if obj.get('part_id') == 'top_caveman']
    assert len(top) == 1
    review.import_part(PARTS / 'fitted/hair_crop.glb', rig)
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 12 if args.quick else 32
    scene.world = bpy.data.worlds.new('Caveman fitting background')
    scene.world.color = (.18, .18, .18)
    for location, energy, size in [((-3, -4, 5), 600, 4), ((4, -2, 3), 450, 4), ((0, 3, 4), 650, 3)]:
        bpy.ops.object.light_add(type='AREA', location=location)
        light = bpy.context.object
        light.data.energy, light.data.shape, light.data.size = energy, 'DISK', size
        light.rotation_euler = (Vector((0, 0, 1.3)) - light.location).to_track_quat('-Z', 'Y').to_euler()
    bpy.ops.object.camera_add()
    camera = bpy.context.object
    camera.data.type = 'ORTHO'
    scene.camera = camera
    scene.render.resolution_x, scene.render.resolution_y = 700, 900
    scene.render.resolution_percentage = 70 if args.quick else 100
    scene.view_settings.view_transform = 'AgX'

    def render(name, location, target, scale):
        camera.location = location
        camera.rotation_euler = (Vector(target) - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(images / f'{name}.png')
        bpy.ops.render.render(write_still=True)

    for name, location in [('front', (0, -5, 1.45)), ('side', (-5, 0, 1.45)), ('back', (0, 5, 1.45))]:
        render(name, location, (0, 0, 1.46), 1.08)
    review.contact_sheet(images, ['front', 'side', 'back'], ['FRONT', 'RIGHT SHOULDER', 'BACK'], 3, Path(str(PREFIX) + '-rest.png'))
    conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
    ordered = sorted(rig.pose.bones, key=lambda bone: len(bone.parent_recursive))
    for bone in rig.pose.bones:
        bone.rotation_mode = 'QUATERNION'

    def key_pose(frame):
        for bone in rig.pose.bones:
            for channel in ['location', 'rotation_quaternion', 'scale']:
                bone.keyframe_insert(data_path=channel, frame=frame)

    if not args.rest_only:
        animation = json.loads((ROOT / 'doc/assets/modular-caveman-tripo-top-animation-v1.json').read_text())
        for source in animation['sources']:
            assert review.file_record(ROOT / source['path'])['sha256'] == source['sha256']
        key_pose(1)
        scene.timeline_markers.new('REST', frame=1)
        snapshots = json.loads((OUTPUT / 'animation-snapshots.json').read_text())
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
            depsgraph = bpy.context.evaluated_depsgraph_get()
            bounds = []
            for obj in top:
                evaluated = obj.evaluated_get(depsgraph)
                bounds.extend(evaluated.matrix_world @ v.co for v in evaluated.data.vertices)
            center = Vector(tuple((min(v[i] for v in bounds) + max(v[i] for v in bounds)) / 2 for i in range(3)))
            render(snapshot['clip'], center + Vector((-2.3, -5, .9)), center + Vector((0, 0, .08)), 1.10)
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
        review.contact_sheet(images, names, ['IDLE', 'WALK', 'RUN', 'JUMP', 'SLASH', 'SIT'], 3, Path(str(PREFIX) + '-motion.png'))
    render('hero', (-2.4, -5, 1.75), (0, 0, 1.48), 1.10)
    if not args.quick:
        shutil.copyfile(images / 'hero.png', Path(str(PREFIX) + '-hero.png'))
        references = bpy.data.collections.new('Unmodified Tripo source - hidden')
        scene.collection.children.link(references)
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        for obj in set(bpy.data.objects) - before:
            for collection in list(obj.users_collection):
                collection.objects.unlink(obj)
            references.objects.link(obj)
        references.hide_render = True
        references.hide_viewport = True
        bpy.ops.object.select_all(action='DESELECT')
        top[0].select_set(True)
        bpy.context.view_layer.objects.active = top[0]
        for area in bpy.context.screen.areas:
            if area.type == 'VIEW_3D':
                area.spaces.active.region_3d.view_distance = 1.8
                area.spaces.active.region_3d.view_location = Vector((0, 0, 1.45))
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'caveman-top-fitting.blend'))
        report = dict(date='2026-10-04', status='User-selected fitting restored; rigged and reviewed with seven sampled game animations. Extreme-pose collisions and gameplay acceptance remain pending.',
                      blender_version=bpy.app.version_string, skin_mask='No top skin masking; only alternate boot-ankle meshes hidden',
                      user_selected_model_sha256='e495478ffdcf1d018a9c1faed604d91e92b6b2df91e1baf1599544800ad9ab9f',
                      budget=dict(top=2110, base_including_hidden=13891, base_visible=13520, crop_hair=933,
                                  assembled_including_hidden=16934, assembled_visible=16563, face=1505,
                                  scope='Base, crop hair and top only; pants, bracer, boots and weapons not included'),
                      editable=review.file_record(OUTPUT / 'caveman-top-fitting.blend'),
                      images=[review.file_record(Path(str(PREFIX) + suffix)) for suffix in ['-rest.png', '-motion.png', '-hero.png']],
                      fitting=review.file_record(ROOT / 'doc/assets/modular-caveman-tripo-top-fitting-v1.json'),
                      animation=review.file_record(ROOT / 'doc/assets/modular-caveman-tripo-top-animation-v1.json'))
        (ROOT / 'doc/assets/modular-caveman-tripo-top-fitted-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='caveman-top-review-') as directory:
        main(Path(directory))
