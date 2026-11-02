import json
from pathlib import Path
import sys

import bpy
import bmesh
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = ROOT / 'assets/items/valkyrie_gloves/fitted_v3'
bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete(use_global=False)
bpy.ops.import_scene.gltf(filepath=str(ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'))
armature = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
body = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
reference = body.copy()
reference.data = body.data.copy()
reference.name = 'female_body_uncut_reference'
bpy.context.collection.objects.link(reference)
reference.hide_render = True
reference.hide_set(True)
mask = json.loads((DIRECTORY / 'body-occlusion.json').read_text())
bm = bmesh.new()
bm.from_mesh(body.data)
bm.faces.ensure_lookup_table()
body_faces = [face for face in bm.faces if face.material_index == 0]
if len(body_faces) != mask['primitive_triangles']:
    raise ValueError('Unexpected body topology')
bmesh.ops.delete(bm, geom=[body_faces[i] for i in mask['hide_triangle_indices']], context='FACES')
bm.to_mesh(body.data)
bm.free()
existing = set(bpy.context.scene.objects)
bpy.ops.import_scene.gltf(filepath=str(DIRECTORY / 'bracers_female_valkyrie.glb'))
imported = set(bpy.context.scene.objects) - existing
for obj in imported:
    if obj.type != 'MESH':
        continue
    world = obj.matrix_world.copy()
    obj.parent = armature
    obj.matrix_world = world
    for modifier in obj.modifiers:
        if modifier.type == 'ARMATURE':
            modifier.object = armature
for obj in imported:
    if obj.type != 'MESH':
        bpy.data.objects.remove(obj, do_unlink=True)
scene = bpy.context.scene
scene.unit_settings.system = 'METRIC'
scene.unit_settings.scale_length = 1
bpy.ops.wm.save_as_mainfile(filepath=str(DIRECTORY / 'fitting.blend'))
if '--render' in sys.argv:
    bpy.ops.object.light_add(type='AREA', location=(0, -3, 3))
    bpy.context.object.data.energy = 500
    bpy.context.object.data.size = 5
    bpy.ops.object.camera_add(location=(0, -3, 1.4))
    camera = bpy.context.object
    camera.rotation_euler = (Vector((0, 0, 1.2)) - camera.location).to_track_quat('-Z', 'Y').to_euler()
    camera.data.type = 'ORTHO'
    camera.data.ortho_scale = 1.3
    scene.camera = camera
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 16
    scene.world.color = (0.35, 0.35, 0.35)
    scene.render.resolution_x = 1100
    scene.render.resolution_y = 700
    scene.render.resolution_percentage = 100
    scene.render.filepath = str(DIRECTORY / 'preview.png')
    bpy.ops.render.render(write_still=True)
