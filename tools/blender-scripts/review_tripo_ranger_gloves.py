"""Review the delivered ranger glove and its mirrored fitting."""
import argparse
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

import bpy
import bmesh
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01'
OUTPUT = PARTS / 'ranger/tripo_gloves_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_male_01/parts/ranger'


def helper(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


studio = helper('studio', 'tools/blender-scripts/review_tripo_pants.py')
review = studio.review


def main(directory):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--raw', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(OUTPUT / ('source.glb' if args.raw else 'gloves_ranger.glb')))
    meshes = [o for o in bpy.data.objects if o.type == 'MESH']
    if args.raw:
        diagnostic, low, high = studio.source_review.diagnostics(meshes)
        report = dict(source=review.file_record(OUTPUT / 'source.glb'),
                      status='Raw single glove; not fitted or rigged', **diagnostic)
        (ROOT / 'doc/assets/modular-ranger-tripo-gloves-source-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')
        center, scale = (low + high) / 2, (high.z - low.z) * 1.12
        views = [('front', (0, -5, 0)), ('back', (0, 5, 0)), ('side', (5, 0, 0))]
        prefix = 'source'
    else:
        rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
        before = set(bpy.data.objects)
        review.import_part(PARTS / 'fitted/base.glb', rig)
        for obj in set(bpy.data.objects) - before:
            if obj.type == 'MESH':
                obj.hide_render = obj.get('region') not in ['hands', 'forearms']
        body = [o for o in set(bpy.data.objects) - before if o.type == 'MESH']
        for obj in body:
            if obj.get('region') != 'forearms':
                continue
            bm = bmesh.new()
            bm.from_mesh(obj.data)
            bm.transform(obj.matrix_world)
            for side in [-1, 1]:
                elbow = Vector((side * .311339, .056036, 1.247962))
                wrist = Vector((side * .445653, .045557, .993593))
                vertices = {v for v in bm.verts if v.co.x * side > .25}
                geometry = list(vertices) + [e for e in bm.edges if any(v in vertices for v in e.verts)]
                geometry += [f for f in bm.faces if any(v in vertices for v in f.verts)]
                bmesh.ops.bisect_plane(bm, geom=geometry, plane_co=elbow.lerp(wrist, .4),
                                      plane_no=wrist - elbow, clear_outer=True)
            bm.transform(obj.matrix_world.inverted())
            bm.to_mesh(obj.data)
            bm.free()
        center, scale = Vector((-.44, .045, 1.015)), .49
        views = [('dorsal', (-4, -.6, 2)), ('palm', (4, .6, -2)), ('side', (0, -5, 0))]
        prefix = 'fitted'
    scene, camera = studio.setup(True)
    for name, offset in views:
        camera.location = center + Vector(offset)
        camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(directory / f'{name}.png')
        bpy.ops.render.render(write_still=True)
    review.contact_sheet(directory, [name for name, _ in views], [name.upper() for name, _ in views], 3,
                         IMAGES / f'tripo-gloves-{prefix}-v1.png')
    if args.raw:
        return

    def render(name, target, offset, scale):
        target = Vector(target)
        camera.location = target + Vector(offset)
        camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(directory / f'{name}.png')
        bpy.ops.render.render(write_still=True)

    for part in ['ranger/tripo_top_v4/top_ranger.glb', 'ranger/tripo_boots_v1/pants_ranger_boots-review.glb',
                 'ranger/tripo_boots_v1/boots_ranger.glb', 'fitted/hair_crop.glb']:
        review.import_part(PARTS / part, rig)
    for obj in body:
        obj.hide_render = obj.get('region') in ['torso', 'upper_arms', 'legs', 'ankles', 'boot_ankles', 'feet']
        if obj.get('region') == 'neck':
            review.clip_top_skin(obj, True)
        obj.hide_set(obj.hide_render)
    for name, offset in [('front', (0, -5, 0)), ('back', (0, 5, 0)), ('side', (5, 0, 0))]:
        render(name, (0, 0, .97), offset, 2.08)
    review.contact_sheet(directory, ['front', 'back', 'side'], ['FRONT', 'BACK', 'SIDE'], 3,
                         IMAGES / 'tripo-gloves-set-v1.png')
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
        target = rig.matrix_world @ rig.pose.bones['RightHand'].matrix.translation
        render(snapshot['clip'], target + Vector((-.01, 0, -.015)), (-4, -2, 2), .48)
    names = [snapshot['clip'] for snapshot in snapshots] + ['dorsal']
    review.contact_sheet(directory, names, [name.upper() for name in names], 4, IMAGES / 'tripo-gloves-motion-v1.png')
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
    guides = bpy.data.collections.new('Interfaces v1')
    scene.collection.children.link(guides)
    guides.hide_render = True
    data = json.loads((PARTS / 'interfaces/v1/interfaces.json').read_text())
    for entry in data['interfaces']:
        if not entry['name'].startswith(('glove_', 'bracer_')):
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
    references = bpy.data.collections.new('Unmodified Tripo source')
    scene.collection.children.link(references)
    for obj in set(bpy.data.objects) - before:
        for collection in list(obj.users_collection):
            collection.objects.unlink(obj)
        references.objects.link(obj)
    references.hide_render = references.hide_viewport = True
    render('editable-rest', (0, 0, .97), (-3, -5, .7), 2.08)
    bpy.ops.file.pack_all()
    bpy.context.preferences.filepaths.save_version = 0
    target = OUTPUT / 'ranger-gloves-fitting.blend'
    bpy.ops.wm.save_as_mainfile(filepath=str(target))
    report = dict(date='2026-10-07', blender_version=bpy.app.version_string, editable=review.file_record(target),
                  fitting=review.file_record(ROOT / 'doc/assets/modular-ranger-tripo-gloves-fitting-v1.json'),
                  images=[review.file_record(IMAGES / f'tripo-gloves-{kind}-v1.png') for kind in ['source', 'fitted', 'set', 'motion']],
                  scope='Rest front/back/side and seven sampled actual animation poses; editable scene contains canonical rig, outfit, source, textures and connection guides')
    (ROOT / 'doc/assets/modular-ranger-tripo-gloves-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='ranger-gloves-review-') as directory:
        main(Path(directory))
