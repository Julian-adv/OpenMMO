"""Review Tripo trousers against the canonical body and actual game poses."""
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
OUTPUT = PARTS / 'rogue_tripo_pants_v1'
PREFIX = ROOT / 'doc/images/characters/modular_human_male_01/parts/rogue/tripo-pants-v1'
spec = importlib.util.spec_from_file_location('rogue_review', ROOT / 'tools/blender-scripts/review_rogue_fitting.py')
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)
spec = importlib.util.spec_from_file_location('source_review', ROOT / 'tools/blender-scripts/review_outfit_sources.py')
source_review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source_review)


def setup(quick):
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 12 if quick else 24
    scene.world = bpy.data.worlds.new('Trouser review background')
    scene.world.color = (.18, .18, .18)
    for location, energy, size in [((-3, -4, 5), 600, 4), ((4, -2, 3), 450, 4), ((0, 3, 4), 650, 3)]:
        bpy.ops.object.light_add(type='AREA', location=location)
        light = bpy.context.object
        light.data.energy, light.data.shape, light.data.size = energy, 'DISK', size
        light.rotation_euler = (Vector((0, 0, .9)) - light.location).to_track_quat('-Z', 'Y').to_euler()
    bpy.ops.object.camera_add()
    camera = bpy.context.object
    camera.data.type = 'ORTHO'
    scene.camera = camera
    scene.render.resolution_x, scene.render.resolution_y = 700, 900
    scene.render.resolution_percentage = 70 if quick else 100
    scene.view_settings.view_transform = 'AgX'
    return scene, camera


def main(images):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', action='store_true')
    parser.add_argument('--quick', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    bpy.ops.wm.read_factory_settings(use_empty=True)
    if args.source:
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        meshes = [obj for obj in bpy.data.objects if obj.type == 'MESH']
        diagnostic, _, _ = source_review.diagnostics(meshes)
        (ROOT / 'doc/assets/modular-rogue-tripo-pants-source-review.json').write_text(json.dumps(dict(
            status='Raw source review; not fitted or rigged', source=review.file_record(OUTPUT / 'source.glb'),
            **diagnostic), indent=2) + '\n')
    else:
        animation = json.loads((ROOT / 'doc/assets/modular-rogue-tripo-pants-animation-v1.json').read_text())
        for source in animation['sources']:
            assert review.file_record(ROOT / source['path'])['sha256'] == source['sha256']
        bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
        rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
        body = [obj for obj in bpy.data.objects if obj.type == 'MESH']
        meshes = review.import_part(OUTPUT / 'pants_rogue.glb', rig)
        appearance = body + meshes
        for path in ['rogue_tripo_v1/top_rogue.glb', 'rogue_fitted_v8/gloves_rogue.glb',
                     'rogue_fitted_v8/boots_rogue.glb', 'fitted/hair_crop.glb']:
            appearance.extend(review.import_part(PARTS / path, rig))
        for obj in body:
            review.clip_top_skin(obj, True)
            obj.hide_render = obj.get('region') in ['upper_arms', 'boot_ankles', 'legs', 'ankles', 'feet']
            obj.hide_set(obj.hide_render)
    scene, camera = setup(args.quick)

    def render(name, location, target, scale):
        camera.location = location
        camera.rotation_euler = (Vector(target) - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(images / f'{name}.png')
        bpy.ops.render.render(write_still=True)

    center, scale = ((0, 0, .5), 1.15) if args.source else ((0, 0, .65), 1.48)
    for name, location in [('front', (0, -5, .7)), ('back', (0, 5, .7)), ('side', (5, 0, .7))]:
        render(name, location, center, scale)
    sheet = Path(str(PREFIX) + ('-source.png' if args.source else '-rest.png'))
    review.contact_sheet(images, ['front', 'back', 'side'], ['FRONT', 'BACK', 'SIDE'], 3, sheet)
    if args.source:
        return
    for obj in body:
        if obj.get('region') in ['legs', 'ankles']:
            obj.hide_render = False
    render('skin-front', (0, -5, .7), (0, 0, .65), 1.48)
    render('skin-back', (0, 5, .7), (0, 0, .65), 1.48)
    review.contact_sheet(images, ['skin-front', 'skin-back'], ['SKIN VISIBLE FRONT', 'SKIN VISIBLE BACK'], 2, Path(str(PREFIX) + '-skin.png'))
    for obj in body:
        if obj.get('region') in ['legs', 'ankles']:
            obj.hide_render = True
    render('hero', (2.5, -5, 1.6), (0, 0, .99), 2.13)
    render('waist', (1.5, -5, 1.4), (0, 0, 1.1), .7)
    render('ankles', (1.5, -5, .45), (0, 0, .23), .58)
    review.contact_sheet(images, ['hero', 'waist', 'ankles'], ['FULL SET', 'WAIST', 'ANKLES'], 3, Path(str(PREFIX) + '-connections.png'))
    conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
    to_blender = rig.matrix_world.inverted() @ conversion
    from_blender = conversion.inverted() @ rig.matrix_world
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
                bone.matrix = to_blender @ deformation @ from_blender @ bone.bone.matrix_local
                bpy.context.view_layer.update()
        key_pose(frame)
        scene.timeline_markers.new(snapshot['clip'], frame=frame)
        depsgraph = bpy.context.evaluated_depsgraph_get()
        bounds = []
        for obj in appearance:
            if obj.hide_render:
                continue
            evaluated = obj.evaluated_get(depsgraph)
            evaluated_mesh = evaluated.to_mesh()
            bounds.extend(evaluated.matrix_world @ v.co for v in evaluated_mesh.vertices)
            evaluated.to_mesh_clear()
        center = Vector(tuple((min(v[i] for v in bounds) + max(v[i] for v in bounds)) / 2 for i in range(3)))
        location = center + Vector((2.3, -5, 1.2))
        rotation = (center - location).to_track_quat('-Z', 'Y').to_matrix().transposed()
        projected = [rotation @ (p - center) for p in bounds]
        width = max(abs(p.x) for p in projected) * 2
        height = max(abs(p.y) for p in projected) * 2
        scale = max(1.65, height * 1.15, width * 900 / 700 * 1.15)
        render(snapshot['clip'], location, center, scale)
        if snapshot['clip'] == 'sit_idle':
            render('sit-back', center + Vector((0, 5, 1.2)), center, scale)
    names = [p['clip'] for p in snapshots if p['clip'] != 'combat_idle']
    review.contact_sheet(images, names, ['IDLE', 'WALK', 'RUN', 'JUMP', 'SLASH', 'SIT'], 3, Path(str(PREFIX) + '-motion.png'))
    review.contact_sheet(images, ['sit_idle', 'sit-back'], ['SIT FRONT', 'SIT BACK'], 2, Path(str(PREFIX) + '-sit.png'))
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
    if args.quick:
        return
    references = bpy.data.collections.new('Unmodified source and canonical body - hidden')
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
    meshes[0].select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.file.pack_all()
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'tripo-pants-fitting.blend'))
    report = dict(status='Fitted candidate; full gameplay and mixed-equipment acceptance pending',
        blender_version=bpy.app.version_string,
        editable=review.file_record(OUTPUT / 'tripo-pants-fitting.blend'),
        images=[review.file_record(Path(str(PREFIX) + suffix)) for suffix in ['-source.png', '-rest.png', '-skin.png', '-connections.png', '-motion.png', '-sit.png']],
        fitting=review.file_record(ROOT / 'doc/assets/modular-rogue-tripo-pants-fitting-v1.json'),
        animation=review.file_record(ROOT / 'doc/assets/modular-rogue-tripo-pants-animation-v1.json'),
        skin_mask=dict(hidden_regions=['upper_arms', 'boot_ankles', 'legs', 'ankles', 'feet'], torso_maximum_y=1.14,
            scope='Review scene only; canonical body unchanged'))
    (ROOT / 'doc/assets/modular-rogue-tripo-pants-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='tripo-pants-review-') as directory:
        main(Path(directory))
