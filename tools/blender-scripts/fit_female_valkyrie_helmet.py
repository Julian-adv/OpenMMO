import hashlib
import json
from pathlib import Path
import sys

import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/items/valkyrie_helmet/source/model.glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
HAIR = ROOT / 'assets/modular_human_female_01/hair/tripo_warrior_v1/fitted_v1/hair_female_warrior.glb'
OUT = ROOT / 'assets/items/valkyrie_helmet/fitted_v2'
OUT.mkdir(parents=True, exist_ok=True)
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != '0d437489781fc9c6b825997e909160ea2b70dbdf02c4fa830680ba944f58f0d7':
    raise ValueError('Unexpected source model')
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(BODY))
rig = next(obj for obj in bpy.context.scene.objects if obj.type == 'ARMATURE')
rig['rig_id'] = 'human_female_01_local_v1'
if rig.animation_data:
    rig.animation_data.action = None
    for track in rig.animation_data.nla_tracks:
        track.mute = True
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.scene.frame_set(0)
bpy.context.view_layer.update()
before = set(bpy.context.scene.objects)
bpy.ops.import_scene.gltf(filepath=str(HAIR))
hair_objects = set(bpy.context.scene.objects) - before
for obj in hair_objects:
    if obj.type == 'MESH':
        world = obj.matrix_world.copy()
        obj.parent = rig
        obj.matrix_world = world
        for modifier in obj.modifiers:
            if modifier.type == 'ARMATURE':
                modifier.object = rig
for obj in hair_objects:
    if obj.type != 'MESH':
        bpy.data.objects.remove(obj, do_unlink=True)
before = set(bpy.context.scene.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
imported = set(bpy.context.scene.objects) - before
helmet = next(obj for obj in imported if obj.name == 'tripo_part_6')
for obj in imported:
    if obj != helmet:
        bpy.data.objects.remove(obj, do_unlink=True)
helmet.name = 'helmet_female_valkyrie'
bpy.ops.object.select_all(action='DESELECT')
helmet.select_set(True)
bpy.context.view_layer.objects.active = helmet
bpy.ops.export_scene.gltf(filepath=str(SOURCE.parent / 'helmet-extracted.glb'), export_format='GLB', use_selection=True, export_animations=False)
helmet.data.transform(Matrix.Translation((0, 0.012, 0.452)) @ Matrix.Scale(1.4, 4) @ helmet.matrix_world)
helmet.matrix_world = Matrix.Identity(4)
pivot = Vector((0, 0.012, 1.69))
for vertex in helmet.data.vertices:
    vertex.co = pivot + (vertex.co - pivot) * 1.15
    vertex.co.z = pivot.z + (vertex.co.z - pivot.z) * 1.18 - 0.051
points = [tuple(round(value, 6) for value in vertex.co) for vertex in helmet.data.vertices]
neighbors = {point: set() for point in points}
for face in helmet.data.polygons:
    corners = [points[i] for i in face.vertices]
    for a, b in zip(corners, corners[1:] + corners[:1]):
        neighbors[a].add(b)
        neighbors[b].add(a)
remaining = set(neighbors)
front_points = set()
while remaining:
    seed = next(iter(remaining))
    component = {seed}
    pending = [seed]
    remaining.remove(seed)
    while pending:
        current = pending.pop()
        for neighbor in neighbors[current]:
            if neighbor in remaining:
                remaining.remove(neighbor)
                component.add(neighbor)
                pending.append(neighbor)
    if max(abs(point[0]) for point in component) < 0.075:
        front_points.update(component)
front_vertices = [vertex for vertex, point in zip(helmet.data.vertices, points) if point in front_points]
if not front_vertices:
    raise ValueError('No pendant and chain components found')
body = bpy.data.objects['female_body']
body_surface = BVHTree.FromPolygons(
    [body.matrix_world @ vertex.co for vertex in body.data.vertices],
    [face.vertices[:] for face in body.data.polygons],
)
gaps = []
for vertex in front_vertices:
    point = vertex.co
    hit, normal, _, _ = body_surface.ray_cast(Vector((point.x, -1, point.z)), Vector((0, 1, 0)), 2)
    if hit is not None and normal.y < -0.1:
        gaps.append(hit.y - point.y)
if not gaps or min(gaps) <= 0.016:
    raise ValueError('Not enough forehead clearance for the pendant adjustment')
for vertex in front_vertices:
    vertex.co.y += 0.012
pendant_fit = {
    'method': 'Rigid translation of the pendant and complete chain components; side wings unchanged.',
    'translation_blender_m': [0, 0.012, 0],
    'moved_vertices': len(front_vertices),
    'minimum_forehead_gap_before_m': min(gaps),
    'minimum_forehead_gap_after_m': min(gaps) - 0.012,
    'curve_preserved': True,
}
bpy.ops.object.select_all(action='DESELECT')
helmet.select_set(True)
bpy.context.view_layer.objects.active = helmet
group = helmet.vertex_groups.new(name='Head')
group.add(list(range(len(helmet.data.vertices))), 1, 'REPLACE')
helmet.parent = rig
modifier = helmet.modifiers.new('Armature', 'ARMATURE')
modifier.object = rig
helmet['rig_id'] = 'human_female_01_local_v1'
helmet['part_id'] = 'helmet_female_valkyrie'
rig.select_set(True)
bpy.ops.export_scene.gltf(filepath=str(OUT / 'helmet_female_valkyrie.glb'), export_format='GLB', use_selection=True, export_animations=False, export_extras=True)
scene = bpy.context.scene
if scene.world is None:
    scene.world = bpy.data.worlds.new('World')
scene.unit_settings.system = 'METRIC'
scene.unit_settings.scale_length = 1
bpy.ops.wm.save_as_mainfile(filepath=str(OUT / 'female-valkyrie-helmet-fitted.blend'))
report = {
    'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
    'source_node': 'tripo_part_6',
    'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
    'hair_sha256': hashlib.sha256(HAIR.read_bytes()).hexdigest(),
    'triangles': sum(len(face.vertices) - 2 for face in helmet.data.polygons),
    'vertices': len(helmet.data.vertices),
    'rig_id': 'human_female_01_local_v1', 'weight': 'Head: 1.0',
    'initial_scale': 1.4, 'initial_translation_blender_m': [0, 0.012, 0.452],
    'overall_adjustment': 1.15, 'height_adjustment': 1.18, 'vertical_offset_m': -0.051,
    'body_occlusion': 'none', 'hair_policy': 'preserve selected hair',
    'pendant_fit': pendant_fit,
    'status': 'Initial fitting candidate; appearance and mixed equipment need review.',
}
(OUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
(OUT / 'body-occlusion.json').write_text(json.dumps({
    'mesh_index': 0, 'primitive_index': 0, 'primitive_triangles': 20799,
    'hide_triangle_indices': [], 'body_sha256': report['body_sha256'],
}, indent=2) + '\n')
if '--render' in sys.argv:
    bpy.ops.object.light_add(type='AREA', location=(0, -3, 3))
    bpy.context.object.data.energy = 450
    bpy.context.object.data.size = 4
    bpy.ops.object.camera_add(location=(0, -3, 1.78))
    camera = bpy.context.object
    camera.data.type = 'ORTHO'
    camera.data.ortho_scale = 0.7
    scene.camera = camera
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 24
    scene.world.color = (0.35, 0.35, 0.35)
    scene.render.resolution_x = 1000
    scene.render.resolution_y = 1000
    scene.render.resolution_percentage = 100
    for name, location, target in [
        ('front', (0, -3, 1.78), (0, 0, 1.74)),
        ('side', (3, -0.25, 1.78), (0, 0, 1.74)),
        ('back', (0, 3, 1.78), (0, 0, 1.74)),
        ('top', (0, -0.6, 2.7), (0, 0, 1.72)),
    ]:
        camera.location = location
        camera.rotation_euler = (Vector(target) - camera.location).to_track_quat('-Z', 'Y').to_euler()
        scene.render.filepath = str(OUT / f'preview-{name}.png')
        bpy.ops.render.render(write_still=True)
