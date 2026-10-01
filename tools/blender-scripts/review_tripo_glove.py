"""Render a Tripo hand accessory and its canonical-body fitting."""
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
spec = importlib.util.spec_from_file_location('pants_review', ROOT / 'tools/blender-scripts/review_tripo_pants.py')
pants = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pants)
review = pants.review


def main(images):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', action='store_true')
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--part', choices=['glove', 'wrap'], default='glove')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    kind = args.part
    output = PARTS / f'rogue_tripo_{kind}_v1'
    prefix = ROOT / f'doc/images/characters/modular_human_male_01/parts/rogue/tripo-{kind}-v1'
    record = ROOT / f'doc/assets/modular-rogue-tripo-{kind}'
    side = 'Right' if kind == 'glove' else 'Left'
    if not args.source and not args.quick:
        animation = json.loads(Path(str(record) + '-animation-v1.json').read_text())
        for source in animation['sources']:
            assert review.file_record(ROOT / source['path'])['sha256'] == source['sha256']
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(output / ('source.glb' if args.source else 'gloves_rogue.glb')))
    meshes = [obj for obj in bpy.data.objects if obj.type == 'MESH']
    scene, camera = pants.setup(args.quick)

    def render(name, target, offset, scale):
        target = Vector(target)
        camera.location = target + Vector(offset)
        camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(images / f'{name}.png')
        bpy.ops.render.render(write_still=True)

    if args.source:
        diagnostic, low, high = pants.source_review.diagnostics(meshes)
        Path(str(record) + '-source-review.json').write_text(json.dumps(dict(
            status='Raw Tripo source; not fitted or rigged', source=review.file_record(output / 'source.glb'), **diagnostic), indent=2) + '\n')
        for name, offset in [('front', (0, -5, 0)), ('back', (0, 5, 0)), ('side', (5, 0, 0))]:
            render(name, (low + high) / 2, offset, 1.2)
        review.contact_sheet(images, ['front', 'back', 'side'], ['FRONT / RAW', 'BACK / RAW', 'SIDE / RAW'], 3, Path(str(prefix) + '-source.png'))
        return
    rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
    bpy.ops.object.select_all(action='DESELECT')
    before = set(bpy.data.objects)
    review.import_part(PARTS / 'fitted/base.glb', rig)
    body = [obj for obj in set(bpy.data.objects) - before if obj.type == 'MESH']
    for obj in body:
        obj.hide_render = obj.get('region') not in ['hands', 'forearms']
    target = Vector((-.49, .045, .90) if kind == 'glove' else (.423, .045, 1.04))
    for name, offset in [('dorsal', (-4, -1, 2)), ('palm', (4, 1, -2)), ('cuff', (-3, -3, 3))]:
        render(name, target, offset, .31)
    review.contact_sheet(images, ['dorsal', 'palm', 'cuff'], [f'{side.upper()} WRIST / OUTER', f'{side.upper()} WRIST / INNER', 'WRIST / SKIN VISIBLE'], 3, Path(str(prefix) + '-hand.png'))
    for path in ['rogue_tripo_v1/top_rogue.glb', 'rogue_tripo_pants_v1/pants_rogue.glb', 'rogue_fitted_v8/boots_rogue.glb', 'fitted/hair_crop.glb']:
        review.import_part(PARTS / path, rig)
    if kind == 'wrap':
        for obj in bpy.data.objects:
            if obj.type == 'MESH' and obj.get('part_id') == 'pants_rogue':
                review.clip_above(obj, 1.105)
    for obj in body:
        review.clip_top_skin(obj, True, waist_height=1.105 if kind == 'wrap' else 1.14)
        obj.hide_render = obj.get('region') in ['upper_arms', 'boot_ankles', 'legs', 'ankles', 'feet']
        obj.hide_set(obj.hide_render)
    for name, offset in [('front', (0, -5, 0)), ('back', (0, 5, 0)), ('hero', (-3, -5, 1))]:
        render(name, (0, 0, .99), offset, 2.13)
    review.contact_sheet(images, ['front', 'back', 'hero'], ['FRONT', 'BACK', 'FULL SET'], 3, Path(str(prefix) + '-rest.png'))
    if args.quick:
        return
    if (output / 'animation-snapshots.json').exists():
        snapshots = json.loads((output / 'animation-snapshots.json').read_text())
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
            center = rig.matrix_world @ rig.pose.bones[side + 'Hand'].matrix.translation
            offset = (-.015, 0, -.07) if kind == 'glove' else (-.025, 0, .045)
            render(snapshot['clip'], center + Vector(offset), (-4, -2, 2), .4)
        names = [snapshot['clip'] for snapshot in snapshots]
        shutil.copyfile(images / 'dorsal.png', images / 'rest.png')
        names.append('rest')
        review.contact_sheet(images, names, [name.upper() for name in names], 4, Path(str(prefix) + '-motion.png'))
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
    references = bpy.data.collections.new('Unmodified accessory source and canonical body - hidden')
    scene.collection.children.link(references)
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(output / 'source.glb'))
    bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
    for obj in set(bpy.data.objects) - before:
        for collection in list(obj.users_collection):
            collection.objects.unlink(obj)
        references.objects.link(obj)
    references.hide_render = True
    references.hide_viewport = True
    bpy.ops.file.pack_all()
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(output / f'tripo-{kind}-fitting.blend'))
    report = dict(status=f'Workshop {kind} candidate; user appearance and full gameplay review pending', date='2026-10-02',
        blender_version=bpy.app.version_string, editable=review.file_record(output / f'tripo-{kind}-fitting.blend'),
        fitting=review.file_record(Path(str(record) + '-fitting-v1.json')),
        images=[review.file_record(Path(str(prefix) + suffix)) for suffix in ['-source.png', '-hand.png', '-rest.png', '-motion.png']],
        canonical_body_unchanged=True, hand_skin_visible=True)
    Path(str(record) + '-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='tripo-glove-review-') as directory:
        main(Path(directory))
