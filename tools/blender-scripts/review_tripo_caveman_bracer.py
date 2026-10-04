"""Save both fitted bracers on the unchanged reference body with source and guides."""
import importlib.util
import json
from pathlib import Path

import bpy
import bmesh
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
OUTPUT = PARTS / 'caveman_tripo_bracer_v1'
spec = importlib.util.spec_from_file_location('review', ROOT / 'tools/blender-scripts/review_rogue_fitting.py')
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
for obj in bpy.data.objects:
    if obj.type != 'MESH':
        continue
    if obj.get('region') in ('feet', 'ankles', 'boot_ankles'):
        obj.hide_render = True
        obj.hide_set(True)
    elif obj.get('region') == 'legs':
        mesh = bmesh.new()
        mesh.from_mesh(obj.data)
        mesh.transform(obj.matrix_world)
        bmesh.ops.bisect_plane(mesh, geom=list(mesh.verts) + list(mesh.edges) + list(mesh.faces),
            plane_co=(0, 0, .43), plane_no=(0, 0, 1), clear_inner=True)
        mesh.transform(obj.matrix_world.inverted())
        mesh.to_mesh(obj.data)
        mesh.free()
    elif obj.get('region') == 'forearms':
        mesh = bmesh.new()
        mesh.from_mesh(obj.data)
        mesh.transform(obj.matrix_world)
        for side in [-1, 1]:
            elbow = Vector((side * .311339, .056036, 1.247962))
            wrist = Vector((side * .445653, .045557, .993593))
            verts = [v for v in mesh.verts if v.co.x * side > 0]
            edges = [e for e in mesh.edges if all(v.co.x * side > 0 for v in e.verts)]
            faces = [f for f in mesh.faces if all(v.co.x * side > 0 for v in f.verts)]
            normal = (elbow - wrist).normalized()
            bmesh.ops.bisect_plane(mesh, geom=verts + edges + faces,
                plane_co=elbow.lerp(wrist, .23), plane_no=normal, clear_inner=True)
        mesh.transform(obj.matrix_world.inverted())
        mesh.to_mesh(obj.data)
        mesh.free()
bracers = review.import_part(OUTPUT / 'gloves_caveman.glb', rig)
for path in ['caveman_tripo_top_v1/top_caveman.glb', 'caveman_tripo_pants_v1/pants_caveman.glb', 'fitted/hair_crop.glb', 'caveman_tripo_boots_v1/boots_caveman.glb']:
    review.import_part(PARTS / path, rig)
interfaces = json.loads((PARTS / 'interfaces/v1/interfaces.json').read_text())
guides = bpy.data.collections.new('Forearm and wrist interfaces v1')
bpy.context.scene.collection.children.link(guides)
guides.hide_render = True
for interface in interfaces['interfaces']:
    if not interface['name'].startswith(('glove_short_', 'glove_long_')):
        continue
    for name, contour in interface['contours'].items():
        curve = bpy.data.curves.new(interface['name'] + '_' + name, 'CURVE')
        curve.dimensions = '3D'
        curve.bevel_depth = .001
        spline = curve.splines.new('POLY')
        spline.points.add(len(contour['points']) - 1)
        for point, (x, y, z) in zip(spline.points, contour['points']):
            point.co = (x, -z, y, 1)
        spline.use_cyclic_u = True
        guides.objects.link(bpy.data.objects.new(curve.name, curve))
references = bpy.data.collections.new('Original Tripo bracer - unmodified hidden source')
bpy.context.scene.collection.children.link(references)
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
for obj in set(bpy.data.objects) - before:
    for collection in list(obj.users_collection):
        collection.objects.unlink(obj)
    references.objects.link(obj)
references.hide_render = True
references.hide_viewport = True
bpy.ops.object.select_all(action='DESELECT')
for obj in bracers:
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
for area in bpy.context.screen.areas:
    if area.type == 'VIEW_3D':
        area.spaces.active.region_3d.view_distance = 1.5
        area.spaces.active.region_3d.view_location = Vector((.38, .04, 1.15))
bpy.context.scene['review_status'] = 'Fitted mirrored bracers on canonical rest rig; final animations reviewed in Three.js preview'
bpy.ops.file.pack_all()
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'caveman-bracer-fitting.blend'))
