"""Render and save the Tripo vest on the canonical body and sampled game poses."""
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
PARTS = ROOT / 'assets/modular_human_male_01'
OUTPUT = PARTS / 'rogue/tripo_v1'
spec = importlib.util.spec_from_file_location('rogue_review', ROOT / 'tools/blender-scripts/review_rogue_fitting.py')
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)


def main(images):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--quick', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    animation = json.loads((ROOT / 'doc/assets/modular-rogue-tripo-animation-v1.json').read_text())
    for source in animation['sources']:
        assert review.file_record(ROOT / source['path'])['sha256'] == source['sha256']
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
    rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
    body = [o for o in bpy.data.objects if o.type == 'MESH']
    for obj in body:
        obj.hide_render = obj.get('region') == 'boot_ankles'
        obj.hide_set(obj.hide_render)
    top = [obj for obj in review.import_part(OUTPUT / 'top_rogue.glb', rig) if obj.get('part_id') == 'top_rogue']
    review.import_part(PARTS / 'fitted/hair_crop.glb', rig)
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 12 if args.quick else 32
    scene.world = bpy.data.worlds.new('Tripo review background')
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

    for name, location in [('front', (0, -5, 1.45)), ('back', (0, 5, 1.45)), ('side', (5, 0, 1.45))]:
        render(name, location, (0, 0, 1.43), 1.13)
    for obj in body:
        review.clip_top_skin(obj, tripo_top=True)
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
        render(snapshot['clip'], center + Vector((2.3, -5, .9)), center + Vector((0, 0, .08)), 1.35)
    for layer in rig.animation_data.action.layers:
        for strip in layer.strips:
            for slot in rig.animation_data.action.slots:
                channels = strip.channelbag(slot)
                if channels:
                    for curve in channels.fcurves:
                        for key in curve.keyframe_points:
                            key.interpolation = 'CONSTANT'
    rig.animation_data.action.name = 'Sampled game poses - not continuous animation'
    scene.frame_end = len(snapshots) * 10
    scene.frame_set(1)
    for obj in body:
        obj.hide_render = obj.get('region') == 'boot_ankles'
        obj.hide_set(obj.hide_render)
    render('hero', (2.4, -5, 1.65), (0, 0, 1.44), 1.12)
    prefix = ROOT / 'doc/images/characters/modular_human_male_01/parts/rogue/tripo-v1'
    review.contact_sheet(images, ['front', 'back', 'side'], ['FRONT - BODY VISIBLE', 'BACK', 'SIDE'], 3, Path(str(prefix) + '-rest.png'))
    names = [p['clip'] for p in snapshots if p['clip'] != 'combat_idle']
    review.contact_sheet(images, names, ['IDLE', 'WALK', 'RUN', 'JUMP', 'SLASH', 'SIT'], 3, Path(str(prefix) + '-motion.png'))
    if not args.quick:
        shutil.copyfile(images / 'hero.png', Path(str(prefix) + '-hero.png'))
        for obj in body:
            obj.hide_render = obj.get('region') in ['boot_ankles', 'upper_arms']
            obj.hide_set(obj.hide_render)
        references = bpy.data.collections.new('Unmodified Tripo source and canonical body - hidden')
        scene.collection.children.link(references)
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
        for obj in set(bpy.data.objects) - before:
            for collection in list(obj.users_collection):
                collection.objects.unlink(obj)
            references.objects.link(obj)
        references.hide_render = True
        references.hide_viewport = True
        with bpy.data.libraries.load(str(PARTS / 'interfaces/v1/outfit-reference.blend'), link=False) as (source, target):
            target.objects = [name for name in source.objects if name.endswith(('_center', '_band_minus', '_band_plus'))]
        interfaces = bpy.data.collections.new('Canonical body interfaces - hidden')
        scene.collection.children.link(interfaces)
        for obj in target.objects:
            if obj:
                interfaces.objects.link(obj)
        interfaces.hide_render = True
        interfaces.hide_viewport = True
        bpy.ops.object.select_all(action='DESELECT')
        top[0].select_set(True)
        bpy.context.view_layer.objects.active = top[0]
        for area in bpy.context.screen.areas:
            if area.type == 'VIEW_3D':
                area.spaces.active.region_3d.view_distance = 1.8
                area.spaces.active.region_3d.view_location = Vector((0, 0, 1.35))
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'tripo-fitting.blend'))
        report = dict(status='Fitted and rigged candidate; mixed equipment and gameplay acceptance pending',
                      blender_version=bpy.app.version_string, skin_mask=dict(hidden_regions=['upper_arms', 'boot_ankles'], torso_maximum_y=1.14, neck_minimum_y=1.54, neck_maximum_abs_x=.075, scope='Review scene only; canonical base GLB unchanged'), editable=review.file_record(OUTPUT / 'tripo-fitting.blend'),
                      images=[review.file_record(Path(str(prefix) + suffix)) for suffix in ['-rest.png', '-motion.png', '-hero.png']],
                      fitting=review.file_record(ROOT / 'doc/assets/modular-rogue-tripo-fitting-v1.json'),
                      animation=review.file_record(ROOT / 'doc/assets/modular-rogue-tripo-animation-v1.json'))
        (ROOT / 'doc/assets/modular-rogue-tripo-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='tripo-review-') as directory:
        main(Path(directory))
