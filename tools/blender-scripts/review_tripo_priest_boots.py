"""Save editable priest boots on the current body with unmodified source."""
import importlib.util
import json
from pathlib import Path

import bpy

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
OUTPUT = PARTS / 'priest_tripo_boots_v1'


def helper(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


review = helper('review', 'tools/blender-scripts/review_rogue_fitting.py')
raw = helper('raw', 'tools/blender-scripts/review_outfit_sources.py')
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
for obj in bpy.data.objects:
    if obj.type == 'MESH' and obj.get('region') in ('legs', 'ankles', 'boot_ankles', 'feet'):
        obj.hide_render = True
        obj.hide_set(True)
boots = review.import_part(OUTPUT / 'boots_priest.glb', rig)
for obj in boots[:]:
    if not obj.name.startswith('priest_boot_'):
        boots.remove(obj)
        bpy.data.objects.remove(obj, do_unlink=True)
diagnostic, _, _ = raw.diagnostics(boots)
references = bpy.data.collections.new('Unmodified Tripo source - hidden')
bpy.context.scene.collection.children.link(references)
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
for obj in set(bpy.data.objects) - before:
    for collection in list(obj.users_collection):
        collection.objects.unlink(obj)
    references.objects.link(obj)
references.hide_render = references.hide_viewport = True
bpy.ops.object.select_all(action='DESELECT')
for obj in boots:
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
for area in bpy.context.screen.areas:
    if area.type == 'VIEW_3D':
        area.spaces.active.region_3d.view_distance = 1.2
        area.spaces.active.region_3d.view_location = (0, 0, .18)
bpy.context.scene['review_status'] = 'Editable fitted boots on canonical body; outfit and motions reviewed in browser'
bpy.ops.file.pack_all()
bpy.context.preferences.filepaths.save_version = 0
editable = OUTPUT / 'priest-boots-fitting-v1.blend'
bpy.ops.wm.save_as_mainfile(filepath=str(editable))
report = dict(status='Fitted mirrored boots; open shaft liners, unchanged current body and hidden original source',
              editable=review.file_record(editable), glb=review.file_record(OUTPUT / 'boots_priest.glb'), **diagnostic)
(ROOT / 'doc/assets/modular-priest-tripo-boots-review-v1.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
