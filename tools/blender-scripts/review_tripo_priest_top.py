"""Render the priest robe source or its fitting candidate."""
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
PARTS = ROOT / 'assets/modular_human_male_01/parts'
OUTPUT = PARTS / 'priest_tripo_top_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_male_01/parts/priest'


def helper(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


review = helper('outfit_review', 'tools/blender-scripts/review_rogue_fitting.py')
raw_review = helper('source_review', 'tools/blender-scripts/review_outfit_sources.py')
glb = helper('glb_io', 'tools/lib/glb.py')


def main(directory):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--raw', action='store_true')
    parser.add_argument('--masked', action='store_true')
    parser.add_argument('--animations', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    bpy.ops.wm.read_factory_settings(use_empty=True)
    body, context, rig = [], [], None
    if args.raw:
        path = OUTPUT / 'source.glb'
        bpy.ops.import_scene.gltf(filepath=str(path))
        top = [obj for obj in bpy.data.objects if obj.type == 'MESH']
        diagnostic, low, high = raw_review.diagnostics(top)
        center = (low + high) / 2
        scale = max(high.z - low.z, (high.x - low.x) / (700 / 900)) * 1.13
        prefix = 'tripo-top-raw-review-v1'
    else:
        path = OUTPUT / 'top_priest.glb'
        bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
        rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
        body = [obj for obj in bpy.data.objects if obj.type == 'MESH']
        for obj in body:
            obj.hide_render = obj.get('region') == 'boot_ankles'
            obj.hide_set(obj.hide_render)
        top = review.import_part(path, rig)
        context.extend(review.import_part(PARTS / 'fitted/hair_crop.glb', rig))
        context.extend(review.import_part(PARTS / 'fitted/pants_cloth.glb', rig))
        diagnostic, _, _ = raw_review.diagnostics(top)
        center, scale = Vector((0, 0, .98)), 2.12
        prefix = 'tripo-top-fitted-v1'
        if args.masked:
            prefix += '-masked'
            for obj in body:
                region = obj.get('region')
                obj.hide_render = region in ['boot_ankles', 'legs', 'upper_arms', 'forearms']
                obj.hide_set(obj.hide_render)
                if region == 'torso':
                    review.clip_above(obj, 1.055)
                if region == 'neck':
                    mesh = bmesh.new()
                    mesh.from_mesh(obj.data)
                    mesh.transform(obj.matrix_world)
                    bmesh.ops.bisect_plane(mesh, geom=list(mesh.verts) + list(mesh.edges) + list(mesh.faces),
                                          plane_co=(0, 0, 1.605), plane_no=(0, 0, 1), clear_inner=True)
                    mesh.transform(obj.matrix_world.inverted())
                    mesh.to_mesh(obj.data)
                    mesh.free()
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 16
    scene.world = bpy.data.worlds.new('Priest review background')
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
    scene.render.resolution_percentage = 100
    scene.view_settings.view_transform = 'AgX'
    IMAGES.mkdir(parents=True, exist_ok=True)
    views = [('front', (0, -5, 0)), ('side', (-5, 0, 0)), ('back', (0, 5, 0))]
    for name, offset in views:
        camera.location = center + Vector(offset)
        camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(directory / f'{name}.png')
        bpy.ops.render.render(write_still=True)
    image = IMAGES / f'{prefix}.png'
    review.contact_sheet(directory, [v[0] for v in views], ['FRONT', 'SIDE', 'BACK'], 3, image)
    report = dict(date='2026-10-08', source=review.file_record(path), image=review.file_record(image),
                  status='Raw Tripo mesh; not fitted' if args.raw else 'Exported skinning baseline; runtime auxiliary robe bones are shown separately in browser reviews',
                  blender_version=bpy.app.version_string, **diagnostic)
    if not args.raw and args.animations:
        animation_report = ROOT / 'doc/assets/modular-priest-tripo-top-animation-v1.json'
        animation = json.loads(animation_report.read_text())
        for source in animation['sources']:
            assert review.file_record(ROOT / source['path'])['sha256'] == source['sha256']
        snapshots = json.loads((OUTPUT / 'animation-snapshots.json').read_text())
        conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
        ordered = sorted(rig.pose.bones, key=lambda bone: len(bone.parent_recursive))
        names = []
        for pose in snapshots:
            if pose['clip'] == 'combat_idle':
                continue
            for bone in ordered:
                values = pose['bone_deformation_matrices'][bone.name]
                deformation = Matrix([values[i:i + 4] for i in range(0, 16, 4)]).transposed()
                bone.matrix = rig.matrix_world.inverted() @ conversion @ deformation @ conversion.inverted() @ rig.matrix_world @ bone.bone.matrix_local
                bpy.context.view_layer.update()
            graph = bpy.context.evaluated_depsgraph_get()
            bounds = [obj.matrix_world @ Vector(corner) for obj in body + context + top
                      if not obj.hide_render for corner in obj.evaluated_get(graph).bound_box]
            low = Vector(tuple(min(v[i] for v in bounds) for i in range(3)))
            high = Vector(tuple(max(v[i] for v in bounds) for i in range(3)))
            pose_center = (low + high) / 2
            camera.location = pose_center + Vector((0, -5, 0))
            camera.rotation_euler = (pose_center - camera.location).to_track_quat('-Z', 'Y').to_euler()
            camera.data.ortho_scale = max(high.z - low.z, (high.x - low.x) / (700 / 900)) * 1.12
            scene.render.filepath = str(directory / f'{pose["clip"]}.png')
            bpy.ops.render.render(write_still=True)
            names.append(pose['clip'])
        motion = IMAGES / f'{prefix}-motion.png'
        review.contact_sheet(directory, names, ['IDLE', 'WALK', 'RUN', 'JUMP', 'SLASH', 'SIT'], 3, motion)
        report['animation_image'] = review.file_record(motion)
        report['animation_report'] = review.file_record(animation_report)
        report['animation_status'] = 'Exported 65-bone skinning baseline; browser runtime adds eight auxiliary hem bones not included in these images'
        for bone in ordered:
            bone.matrix_basis = Matrix.Identity(4)
        bpy.context.view_layer.update()
    if not args.raw:
        def glb_triangles(path, region=None):
            document, _ = glb.read_glb(path)
            meshes = document['meshes'] if region is None else [document['meshes'][node['mesh']]
                     for node in document['nodes'] if 'mesh' in node and node.get('extras', {}).get('region') == region]
            return sum(document['accessors'][prim['indices']]['count'] // 3 for mesh in meshes for prim in mesh['primitives'])
        canonical_body = glb_triangles(PARTS / 'fitted/base.glb')
        canonical_context = sum(glb_triangles(PARTS / f'fitted/{name}.glb') for name in ['hair_crop', 'pants_cloth'])
        canonical_top = glb_triangles(path)
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
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.file.pack_all()
        blend = OUTPUT / 'priest-top-fitting.blend'
        bpy.ops.wm.save_as_mainfile(filepath=str(blend))
        def triangles(objects, visible=False):
            total = 0
            for obj in objects:
                if visible and obj.hide_render:
                    continue
                obj.data.calc_loop_triangles()
                total += len(obj.data.loop_triangles)
            return total
        report.update(editable=review.file_record(blend),
                      body_mask='Review-scene garment mask only; canonical body and runtime unchanged' if args.masked else 'Only alternate boot_ankles hidden; no garment occlusion mask applied',
                      budget=dict(body=canonical_body, context=canonical_context, top=canonical_top,
                                  assembled_including_hidden=canonical_body + canonical_context + canonical_top,
                                  preview_scene_visible=triangles(body + context + top, True),
                                  head=glb_triangles(PARTS / 'fitted/base.glb', 'head'), face_subset=None,
                                  visible_count_status='Blender preview with temporary skin cuts; final runtime-visible budget pending',
                                  scope='Body, crop hair, existing cloth trousers and priest top; no shoes, gloves, mitre, weapon or cape'))
    report_path = ROOT / f'doc/assets/modular-priest-tripo-top-{"source" if args.raw else "fitted"}-review-v1.json'
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='priest-top-review-') as temporary:
        main(Path(temporary))
