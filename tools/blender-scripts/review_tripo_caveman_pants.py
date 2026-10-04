"""Save the caveman skirt fitting scene; motion physics runs in the preview."""
import importlib.util
import json
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
OUTPUT = PARTS / 'caveman_tripo_pants_v1'
spec = importlib.util.spec_from_file_location('review', ROOT / 'tools/blender-scripts/review_rogue_fitting.py')
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
for obj in bpy.data.objects:
    if obj.type == 'MESH' and obj.get('region') == 'boot_ankles':
        obj.hide_render = True
        obj.hide_set(True)
pants = review.import_part(OUTPUT / 'pants_caveman.glb', rig)
review.import_part(PARTS / 'caveman_tripo_top_v1/top_caveman.glb', rig)
review.import_part(PARTS / 'fitted/hair_crop.glb', rig)
interfaces = json.loads((PARTS / 'interfaces/v1/interfaces.json').read_text())
waist = next(item for item in interfaces['interfaces'] if item['name'] == 'waist')
guides = bpy.data.collections.new('Waist interface v1')
bpy.context.scene.collection.children.link(guides)
guides.hide_render = True
for name, contour in waist['contours'].items():
    curve = bpy.data.curves.new('waist_' + name, 'CURVE')
    curve.dimensions = '3D'
    curve.bevel_depth = .001
    spline = curve.splines.new('POLY')
    spline.points.add(len(contour['points']) - 1)
    for point, (x, y, z) in zip(spline.points, contour['points']):
        point.co = (x, -z, y, 1)
    spline.use_cyclic_u = True
    guides.objects.link(bpy.data.objects.new(curve.name, curve))
references = bpy.data.collections.new('Unmodified Tripo source - hidden')
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
for obj in pants:
    if obj.type == 'MESH':
        obj.select_set(True)
        bpy.context.view_layer.objects.active = obj
for area in bpy.context.screen.areas:
    if area.type == 'VIEW_3D':
        area.spaces.active.region_3d.view_distance = 2.3
        area.spaces.active.region_3d.view_location = Vector((0, 0, 1))
bpy.context.scene['review_status'] = 'Rest fitting only; four separate pelts bend in Three.js runtime, not Blender'
bpy.ops.file.pack_all()
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'caveman-pants-fitting.blend'))
