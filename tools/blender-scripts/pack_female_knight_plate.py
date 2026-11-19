"""Pack the fitted plate armor with the current metre-scale body, skin mask and actions."""
import json
import sys
from pathlib import Path

import bmesh
import bpy

ROOT = Path(__file__).resolve().parents[2]
slot = sys.argv[sys.argv.index('--') + 1] if '--' in sys.argv else 'top'
source = {'top': 'tripo_v1/fitted_v4', 'bottom': 'reference_v1/fitted_v1'}[slot]
OUT = ROOT / f'assets/items/female_knight_{slot}' / source
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'),
                          bone_heuristic='TEMPERANCE')
body = bpy.data.objects['female_body']
rig = body.find_armature()
for track in rig.animation_data.nla_tracks:
    track.mute = True
rig.animation_data.action = None
for bone in rig.pose.bones:
    bone.matrix_basis.identity()

reference = body.copy()
bpy.context.collection.objects.link(reference)
reference.name = 'female_body_unmasked_reference'
reference.hide_render = True
reference.hide_set(True)
body.data = body.data.copy()
mask = json.loads((OUT / 'body-occlusion.json').read_text())
mesh = bmesh.new()
mesh.from_mesh(body.data)
mesh.faces.ensure_lookup_table()
bmesh.ops.delete(mesh, geom=[mesh.faces[i] for i in mask['hide_triangle_indices']], context='FACES')
mesh.to_mesh(body.data)
mesh.free()

before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(OUT / f'{slot}_female_knight.glb'), bone_heuristic='TEMPERANCE')
added = set(bpy.data.objects) - before
part = next(obj for obj in added if obj.type == 'MESH')
world = part.matrix_world.copy()
part.parent = rig
part.matrix_world = world
for modifier in part.modifiers:
    if modifier.type == 'ARMATURE':
        modifier.object = rig
for obj in added - {part}:
    bpy.data.objects.remove(obj, do_unlink=True)
bpy.context.scene.unit_settings.system = 'METRIC'
bpy.context.scene.unit_settings.scale_length = 1
bpy.ops.file.pack_all()
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.wm.save_as_mainfile(filepath=str(OUT / f'female-knight-{slot}-fitted.blend'))
