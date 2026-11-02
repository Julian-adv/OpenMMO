import hashlib
import json
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/items/valkyrie_boots/source/tripo-segmented.glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUT = ROOT / 'assets/items/valkyrie_boots/fitted_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_female_01/parts/valkyrie'
OUT.mkdir(parents=True, exist_ok=True)
IMAGES.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
if rig.animation_data:
    rig.animation_data.action = None
    for track in rig.animation_data.nla_tracks:
        track.mute = True
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.scene.frame_set(0)
bpy.context.view_layer.update()
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
imported = set(bpy.data.objects) - before
boots = next(obj for obj in imported if obj.name == 'tripo_part_3')
for obj in imported:
    if obj != boots:
        bpy.data.objects.remove(obj, do_unlink=True)
boots.data.transform(boots.matrix_world)
boots.matrix_world = Matrix.Identity(4)
boots.name = 'valkyrie_boots_source'
bpy.ops.object.select_all(action='DESELECT')
boots.select_set(True)
bpy.context.view_layer.objects.active = boots
bpy.ops.export_scene.gltf(filepath=str(OUT / 'boots-source.glb'), export_format='GLB', use_selection=True, export_tangents=True)
for v in boots.data.vertices:
    p = v.co
    side = 1 if p.x >= 0 else -1
    height = p.z * 1.8
    t = min(1, max(0, height / .65))
    p.x = p.x * 1.8 + side * (.022 * (1 - t) - .010 * t)
    p.y = p.y * 1.8 + .035 + .035 * t
    p.z = height
boots.data.update()
boots.name = 'boots_female_valkyrie'
bpy.ops.export_scene.gltf(filepath=str(OUT / 'boots-fitted-unskinned.glb'), export_format='GLB', use_selection=True, export_tangents=True)
for action in bpy.data.actions:
    action.use_fake_user = True
bpy.ops.file.pack_all()
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.wm.save_as_mainfile(filepath=str(OUT / 'valkyrie-boots-fitting.blend'))
scene = bpy.context.scene
scene.render.engine = 'BLENDER_WORKBENCH'
scene.display.shading.light = 'STUDIO'
scene.display.shading.color_type = 'TEXTURE'
scene.display.shading.show_shadows = True
scene.display.shading.show_cavity = True
scene.render.resolution_x = 700
scene.render.resolution_y = 800
scene.render.resolution_percentage = 100
bpy.ops.object.camera_add()
cam = bpy.context.object
scene.camera = cam
cam.data.type = 'ORTHO'
cam.data.ortho_scale = 1.1
center = Vector((0, 0, .43))
for name, offset in [('front', (0, -4, 0)), ('side', (4, 0, 0)), ('back', (0, 4, 0))]:
    cam.location = center + Vector(offset)
    cam.rotation_euler = (center - cam.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.filepath = str(IMAGES / f'boots-fit-{name}.png')
    bpy.ops.render.render(write_still=True)
report = {'source': str(SOURCE.relative_to(ROOT)), 'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'source_node': 'tripo_part_3', 'body': str(BODY.relative_to(ROOT)), 'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(), 'triangles': sum(len(p.vertices) - 2 for p in boots.data.polygons), 'status': 'Initial unskinned fitting candidate; fit, skinning, animation, occlusion and runtime compatibility not validated.'}
(OUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
