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
OUTPUT = PARTS / 'priest/tripo_helmet_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_male_01/parts/priest'
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
        (ROOT / 'doc/assets/modular-priest-tripo-helmet-source-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')
        center, scale = (low + high) / 2, (high.z - low.z) * 1.15
    else:
        bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
        rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
        for obj in bpy.data.objects:
            if obj.type == 'MESH':
                obj.hide_render = obj.get('region') not in ['head', 'neck', 'torso', 'upper_arms']
        review.import_part(OUTPUT / 'helmet_priest.glb', rig)
        center, scale = Vector((0, 0, 1.88)), .85
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
                         IMAGES / f'tripo-helmet-{suffix}-v{1 if args.raw else 2}.png')
    if not args.raw:
        for obj in bpy.data.objects:
            if obj.type == 'MESH' and obj.get('part_id') == 'base':
                obj.hide_render = obj.get('region') not in ['head', 'neck', 'hands']
        review.import_part(PARTS / 'priest/tripo_top_v1/top_priest.glb', rig)
        conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
        to_blender = rig.matrix_world.inverted() @ conversion
        from_blender = conversion.inverted() @ rig.matrix_world
        ordered = sorted(rig.pose.bones, key=lambda bone: len(bone.parent_recursive))
        for bone in ordered:
            bone.rotation_mode = 'QUATERNION'
            bone.keyframe_insert(data_path='location', frame=1)
            bone.keyframe_insert(data_path='rotation_quaternion', frame=1)
            bone.keyframe_insert(data_path='scale', frame=1)
        snapshots = json.loads((OUTPUT / 'animation-snapshots.json').read_text())
        views = []
        for index, snapshot in enumerate(snapshots, 1):
            frame = index * 10
            scene.frame_set(frame)
            for bone in ordered:
                values = snapshot['bone_deformation_matrices'].get(bone.name)
                if values is not None:
                    deformation = Matrix([values[i:i + 4] for i in range(0, 16, 4)]).transposed()
                    bone.matrix = to_blender @ deformation @ from_blender @ bone.bone.matrix_local
                    bpy.context.view_layer.update()
                for channel in ['location', 'rotation_quaternion', 'scale']:
                    bone.keyframe_insert(data_path=channel, frame=frame)
            scene.timeline_markers.new(snapshot['clip'], frame=frame)
            head = rig.matrix_world @ rig.pose.bones['Head'].head
            target = head + Vector((0, 0, .16))
            camera.location = target + Vector((3, 5, .6))
            camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
            camera.data.ortho_scale = .90
            name = snapshot['clip']
            views.append(name)
            scene.render.filepath = str(directory / f'{name}.png')
            bpy.ops.render.render(write_still=True)
        views.remove('combat_idle')
        review.contact_sheet(directory, views, [name.upper() for name in views], 3,
                             IMAGES / 'tripo-helmet-motion-v2.png')
        scene.frame_end = len(snapshots) * 10
        scene.frame_set(1)
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
        references = bpy.data.collections.new('Original Tripo mitre')
        scene.collection.children.link(references)
        for obj in set(bpy.data.objects) - before:
            for collection in list(obj.users_collection):
                collection.objects.unlink(obj)
            references.objects.link(obj)
        references.hide_render = references.hide_viewport = True
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'priest-helmet-fitting.blend'))


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='priest-helmet-review-') as temporary:
        main(Path(temporary))
