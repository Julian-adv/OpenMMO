"""Pack editable rogue candidates and render them on the unchanged reference body."""
import argparse
import json
import hashlib
import sys
import subprocess
import tempfile
from pathlib import Path

import bpy
import bmesh
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'


def import_part(path, rig):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
    imported = set(bpy.data.objects) - before
    meshes = [obj for obj in imported if obj.type == 'MESH']
    for obj in meshes:
        world = obj.matrix_world.copy()
        obj.parent = rig
        obj.matrix_world = world
        for modifier in obj.modifiers:
            if modifier.type == 'ARMATURE':
                modifier.object = rig
    for obj in imported:
        if obj.type == 'ARMATURE':
            bpy.data.objects.remove(obj, do_unlink=True)
    return meshes


def contact_sheet(directory, names, labels, columns, output):
    command = ['ffmpeg', '-y', '-loglevel', 'error']
    filters = []
    for index, (name, label) in enumerate(zip(names, labels)):
        command.extend(['-i', str(directory / (name + '.png'))])
        filters.append(f"[{index}:v]scale=350:450,drawtext=text='{label}':x=12:y=15:fontsize=16:fontcolor=white[v{index}]")
    rows = len(names) // columns
    for row in range(rows):
        inputs = ''.join(f'[v{i}]' for i in range(row * columns, (row + 1) * columns))
        filters.append(f'{inputs}hstack=inputs={columns}[r{row}]')
    if rows > 1:
        inputs = ''.join(f'[r{i}]' for i in range(rows))
        filters.append(f'{inputs}vstack=inputs={rows}[out]')
    else:
        filters.append('[r0]null[out]')
    output.parent.mkdir(parents=True, exist_ok=True)
    command.extend(['-filter_complex', ';'.join(filters), '-map', '[out]', '-frames:v', '1', str(output)])
    subprocess.run(command, check=True)


def file_record(path):
    return dict(path=str(path.relative_to(ROOT)), sha256=hashlib.sha256(path.read_bytes()).hexdigest())


def clip_above(obj, height):
    mesh = bmesh.new()
    mesh.from_mesh(obj.data)
    mesh.transform(obj.matrix_world)
    bmesh.ops.bisect_plane(mesh, geom=list(mesh.verts) + list(mesh.edges) + list(mesh.faces),
                          plane_co=(0, 0, height), plane_no=(0, 0, 1), clear_outer=True)
    mesh.transform(obj.matrix_world.inverted())
    mesh.to_mesh(obj.data)
    mesh.free()


def clip_top_skin(obj, tripo_top, waist_height=1.14):
    if obj.get('region') == 'neck':
        mesh = bmesh.new()
        mesh.from_mesh(obj.data)
        mesh.transform(obj.matrix_world)
        bmesh.ops.bisect_plane(mesh, geom=list(mesh.verts) + list(mesh.edges) + list(mesh.faces),
                              plane_co=(0, 0, 1.54 if tripo_top else 1.61), plane_no=(0, 0, 1), clear_inner=True)
        for side in [-1, 1]:
            bmesh.ops.bisect_plane(mesh, geom=list(mesh.verts) + list(mesh.edges) + list(mesh.faces),
                                  plane_co=(side * .075, 0, 0), plane_no=(side, 0, 0), clear_outer=True)
        mesh.transform(obj.matrix_world.inverted())
        mesh.to_mesh(obj.data)
        mesh.free()
    if obj.get('region') == 'torso' and tripo_top:
        clip_above(obj, waist_height)


def main(default_images):
    selection = json.loads((ROOT / 'doc/assets/modular-rogue-source-selection.json').read_text())
    candidate = selection['fitting_candidate']
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / candidate['directory'])
    parser.add_argument('--images', type=Path, default=default_images)
    parser.add_argument('--unmasked', action='store_true')
    parser.add_argument('--quick', action='store_true')
    parser.add_argument('--animations', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:])
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
    rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
    body = [obj for obj in bpy.data.objects if obj.type == 'MESH']
    for name in ['top_rogue', 'pants_rogue', 'gloves_rogue', 'boots_rogue']:
        selected = candidate.get('part_overrides', {}).get(name)
        meshes = import_part(ROOT / selected if selected else args.output / f'{name}.glb', rig)
        if name == 'top_rogue':
            tripo_top = any(obj.get('fitting_status') == 'candidate_tripo_v1' for obj in meshes)
    for obj in body:
        region = obj.get('region')
        if not args.unmasked:
            clip_top_skin(obj, tripo_top)
        obj.hide_render = region == 'boot_ankles' or (not args.unmasked and (region in ['upper_arms', 'legs', 'feet', 'ankles'] or (region == 'torso' and not tripo_top)))
        obj.hide_set(obj.hide_render)
    import_part(PARTS / 'fitted/hair_crop.glb', rig)
    for obj in bpy.data.objects:
        if obj.type == 'MESH':
            for poly in obj.data.polygons:
                poly.use_smooth = True
    if not args.quick:
        references = bpy.data.collections.new('Original outfit sources - unmodified, hidden')
        bpy.context.scene.collection.children.link(references)
        for index, part in enumerate(selection['parts']):
            before = set(bpy.data.objects)
            bpy.ops.import_scene.gltf(filepath=str(ROOT / part['source_glb']))
            for obj in set(bpy.data.objects) - before:
                for collection in list(obj.users_collection):
                    collection.objects.unlink(obj)
                references.objects.link(obj)
                obj.name = 'SOURCE_' + part['id']
                obj.location.x += 3 + index * 2.5
        references.hide_render = True
        references.hide_viewport = True
        with bpy.data.libraries.load(str((ROOT / selection['reference']['interfaces']).with_name('outfit-reference.blend')), link=False) as (source, target):
            target.objects = [name for name in source.objects if name.endswith(('_center', '_band_minus', '_band_plus'))]
            assert len(target.objects) == 36
        interfaces = bpy.data.collections.new('Current outfit interfaces - provisional')
        bpy.context.scene.collection.children.link(interfaces)
        for obj in target.objects:
            if obj:
                interfaces.objects.link(obj)
        interfaces.hide_render = True
        interfaces.hide_viewport = True
    scene = bpy.context.scene
    scene['review_status'] = 'Candidate: provisional body-region hiding only; hand/short-sleeve runtime masks pending'
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 16 if args.quick else 32
    scene.world = bpy.data.worlds.new('Studio background')
    scene.world.color = (.2, .2, .2)
    for loc, energy, size in [((-3, -4, 5), 600, 4), ((4, -2, 3), 450, 4), ((0, 3, 4), 650, 3)]:
        bpy.ops.object.light_add(type='AREA', location=loc)
        light = bpy.context.object
        light.data.energy, light.data.shape, light.data.size = energy, 'DISK', size
        light.rotation_euler = (Vector((0, 0, 1)) - light.location).to_track_quat('-Z', 'Y').to_euler()
    bpy.ops.object.camera_add()
    camera = bpy.context.object
    camera.data.type = 'ORTHO'
    scene.camera = camera
    scene.render.resolution_x, scene.render.resolution_y = 700, 900
    scene.render.resolution_percentage = 75 if args.quick else 100
    scene.view_settings.view_transform = 'AgX'
    args.images.mkdir(parents=True, exist_ok=True)
    views = [('front', (0, -5, 1.05), (0, 0, .98), 2.13),
             ('back', (0, 5, 1.05), (0, 0, .98), 2.13),
             ('side', (5, -.2, 1.1), (0, 0, .98), 2.13),
             ('hands', (-2.8, -2, 1.4), (-.44, .035, 1.02), .48),
             ('connections', (1.9, -4, 1.2), (0, 0, 1.2), 1.0)]
    sleeve_views = [('sleeve-left', (3, -3, 1.6), (.28, 0, 1.36), .64),
                    ('sleeve-right', (-3, -3, 1.6), (-.28, 0, 1.36), .64)]
    for name, location, target, scale in views + sleeve_views:
        camera.location = location
        camera.rotation_euler = (Vector(target) - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(args.images / f'{name}.png')
        bpy.ops.render.render(write_still=True)
    if args.animations:
        review = json.loads((ROOT / candidate['animation_report']).read_text())
        for source in review['sources']:
            assert hashlib.sha256((ROOT / source['path']).read_bytes()).hexdigest() == source['sha256'], 'Re-run animation validation for changed assets'
        snapshots = json.loads((args.output / 'animation-snapshots.json').read_text())
        conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
        ordered = sorted(rig.pose.bones, key=lambda b: len(b.parent_recursive))
        def key_pose(frame):
            for bone in rig.pose.bones:
                for channel in ['location', 'rotation_quaternion', 'scale']:
                    bone.keyframe_insert(data_path=channel, frame=frame)
        for bone in rig.pose.bones:
            bone.rotation_mode = 'QUATERNION'
        key_pose(1)
        scene.timeline_markers.new('REST', frame=1)
        for index, snapshot in enumerate(snapshots, 1):
            frame = index * 10
            scene.frame_set(frame)
            for bone in ordered:
                values = snapshot['bone_deformation_matrices'].get(bone.name)
                if values is None:
                    continue
                deformation = Matrix([values[i:i + 4] for i in range(0, 16, 4)]).transposed()
                bone.matrix = rig.matrix_world.inverted() @ conversion @ deformation @ conversion.inverted() @ rig.matrix_world @ bone.bone.matrix_local
                bpy.context.view_layer.update()
            key_pose(frame)
            scene.timeline_markers.new(snapshot['clip'], frame=frame)
            depsgraph = bpy.context.evaluated_depsgraph_get()
            bounds = []
            for obj in scene.objects:
                if obj.type == 'MESH' and obj.get('part_id') and not obj.hide_render:
                    evaluated = obj.evaluated_get(depsgraph)
                    bounds.extend(evaluated.matrix_world @ Vector(corner) for corner in evaluated.bound_box)
            low = Vector(tuple(min(p[i] for p in bounds) for i in range(3)))
            high = Vector(tuple(max(p[i] for p in bounds) for i in range(3)))
            center = (low + high) / 2
            camera.location = center + Vector((2.5, -5, 1.02))
            camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
            rotation = camera.rotation_euler.to_matrix().transposed()
            projected = [rotation @ (p - center) for p in bounds]
            width = max(abs(p.x) for p in projected) * 2
            height = max(abs(p.y) for p in projected) * 2
            camera.data.ortho_scale = max(2.3, height * 1.15, width * 900 / 700 * 1.15)
            scene.render.filepath = str(args.images / (snapshot['clip'] + '.png'))
            bpy.ops.render.render(write_still=True)
            if snapshot['clip'] == 'sit_idle':
                def mesh_center(name):
                    obj = bpy.data.objects.get(name)
                    if obj is None and tripo_top and name in ['vest_rogue', 'scarf_rogue']:
                        obj = next(o for o in scene.objects if o.type == 'MESH' and o.get('part_id') == 'top_rogue')
                    evaluated = obj.evaluated_get(depsgraph)
                    corners = [evaluated.matrix_world @ Vector(corner) for corner in evaluated.bound_box]
                    return sum(corners, Vector()) / len(corners)
                vest_center = mesh_center('vest_rogue')
                details = [
                    ('sit-back', mesh_center('scarf_rogue'), Vector((2, 4, 1)), .82),
                    ('sit-side', vest_center + Vector((.15, 0, .10)), Vector((4, 1, .6)), .72),
                    ('sit-hem', vest_center - Vector((0, 0, .17)), Vector((2, -4, .1)), .64),
                    ('sit-wrist', mesh_center('wrap_rogue_left'), Vector((2, -3, .5)), .40),
                ]
                for name, target, offset, scale in details:
                    camera.location = target + offset
                    camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
                    camera.data.ortho_scale = scale
                    scene.render.filepath = str(args.images / (name + '.png'))
                    bpy.ops.render.render(write_still=True)
        action = rig.animation_data.action
        action.name = 'Review snapshots - sampled game poses, not continuous playback'
        for layer in action.layers:
            for strip in layer.strips:
                for slot in action.slots:
                    channels = strip.channelbag(slot)
                    if channels:
                        for curve in channels.fcurves:
                            for key in curve.keyframe_points:
                                key.interpolation = 'CONSTANT'
        scene.frame_end = len(snapshots) * 10
        scene.frame_set(1)
        bpy.context.view_layer.update()
    camera.location = (2.8, -5, 2.0)
    camera.rotation_euler = (Vector((0, 0, .98)) - camera.location).to_track_quat('-Z', 'Y').to_euler()
    camera.data.ortho_scale = 2.13
    bpy.ops.object.select_all(action='DESELECT')
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    for area in bpy.context.screen.areas:
        if area.type == 'VIEW_3D':
            area.spaces.active.region_3d.view_distance = 2.8
            area.spaces.active.region_3d.view_location = Vector((0, 0, .95))
    if not args.quick:
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        blend = args.output / 'rogue-fitting.blend'
        bpy.ops.wm.save_as_mainfile(filepath=str(blend))
        rest = ROOT / (candidate['review_image_prefix'] + '-rest.png')
        contact_sheet(args.images, [v[0] for v in views], ['FRONT - CANDIDATE', 'BACK', 'SIDE', 'HAND - CLEANUP PENDING', 'WAIST / UV REVIEW'], 5, rest)
        records = [file_record(rest)]
        sleeves = ROOT / (candidate['review_image_prefix'] + '-sleeves.png')
        contact_sheet(args.images, [v[0] for v in sleeve_views], ['LEFT SLEEVE / LONG GLOVE CUT', 'RIGHT SLEEVE / LONG GLOVE CUT'], 2, sleeves)
        records.append(file_record(sleeves))
        if args.animations:
            motion = ROOT / (candidate['review_image_prefix'] + '-motion.png')
            contact_sheet(args.images, [p['clip'] for p in snapshots], ['IDLE - GAME CLIP SAMPLE', 'WALK', 'RUN', 'JUMP', 'SLASH', 'SIT'], 3, motion)
            records.append(file_record(motion))
            fixes = ROOT / (candidate['review_image_prefix'] + '-fixes.png')
            contact_sheet(args.images, ['sit-back', 'sit-side', 'sit-hem', 'sit-wrist'],
                          ['BACK / SCARF', 'UNDERARM / LAYERS', 'CONTINUOUS HEM', 'WRIST / FOLDED CUFF'], 2, fixes)
            records.append(file_record(fixes))
        record = dict(status=scene['review_status'], blender_version=bpy.app.version_string,
                      editable=file_record(blend), images=records,
                      fitting_record=file_record(ROOT / candidate['report']),
                      animation_record=file_record(ROOT / candidate['animation_report']) if args.animations else None,
                      snapshots=file_record(args.output / 'animation-snapshots.json') if args.animations else None)
        (ROOT / candidate['review_report']).write_text(json.dumps(record, indent=2) + '\n')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='rogue-review-') as directory:
        main(Path(directory))
