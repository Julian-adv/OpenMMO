import hashlib
import json
import math
from pathlib import Path

import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/fantasy warrior 3d model (1).glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUT = ROOT / 'assets/modular_human_female_01/hair/tripo_warrior_v1/fitted_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_female_01/parts/warrior'
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
body = bpy.data.objects['female_body']
points = [body.matrix_world @ v.co for v in body.data.vertices]
surface = BVHTree.FromPolygons(points, [p.vertices[:] for p in body.data.polygons])
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
imported = set(bpy.data.objects) - before
hair = next(obj for obj in imported if obj.name == 'tripo_part_1')
for obj in imported:
    if obj != hair:
        bpy.data.objects.remove(obj, do_unlink=True)
hair.data.transform(Matrix.Translation((0, .012, .452)) @ Matrix.Scale(1.4, 4) @ hair.matrix_world)
hair.matrix_world = Matrix.Identity(4)
moved = []
for vertex in hair.data.vertices:
    point = vertex.co
    if point.z < 1.63 or point.y < -.065:
        continue
    nearest, normal, _, distance = surface.find_nearest(point)
    signed = (point - nearest).dot(normal)
    if signed < .006 and distance < .06:
        height = min(1, max(0, (point.z - 1.63) / .04))
        front = min(1, max(0, (point.y + .065) / .025))
        delta = normal * (.006 - signed) * height * front
        vertex.co += delta
        moved.append(delta.length)
head_center = Vector((0, .012, 1.69))
for vertex in hair.data.vertices:
    vertex.co = head_center + (vertex.co - head_center) * 1.15
    vertex.co.z = head_center.z + (vertex.co.z - head_center.z) * 1.18
    vertex.co.z -= .051
rear_moved = []
for vertex in hair.data.vertices:
    point = vertex.co
    if point.y < .015 or point.z < 1.65:
        continue
    nearest, normal, _, distance = surface.find_nearest(point)
    signed = (point - nearest).dot(normal)
    if signed < .008 and distance < .06:
        rear = min(1, max(0, (point.y - .015) / .025))
        height = min(1, max(0, (point.z - 1.65) / .035))
        delta = normal * (.008 - signed) * rear * height
        vertex.co += delta
        rear_moved.append(delta.length)

def transition(value, low, high):
    return min(1, max(0, (value - low) / (high - low)))

def front_weight(point):
    return (transition(abs(point.x), .06, .085)
            * (1 - transition(point.y, -.055, -.025)))

def required_clearance(point):
    if not (.085 < abs(point.x) < .2 and -.18 < point.y < -.055 and 1.24 < point.z < 1.49):
        return None
    side = 1 if point.x > 0 else -1
    nearest, normal, _, _ = surface.ray_cast(Vector((side * .21, point.y, point.z)),
                                              Vector((-side, 0, 0)), .21)
    if nearest is None or normal.x * side < .15:
        return None
    return side, max(0, (nearest.x - point.x) * side + .006)

constraints = {-1: {}, 1: {}}
for face in hair.data.polygons:
    if len(face.vertices) != 3:
        continue
    vertices = [hair.data.vertices[index].co for index in face.vertices]
    for a, b, c in [(1, 0, 0), (0, 1, 0), (0, 0, 1), (.5, .5, 0),
                    (0, .5, .5), (.5, 0, .5), (1/3, 1/3, 1/3)]:
        point = vertices[0] * a + vertices[1] * b + vertices[2] * c
        result = required_clearance(point)
        if result and result[1] > 0:
            side, distance = result
            key = round(point.z / .002) * .002
            constraints[side][key] = max(constraints[side].get(key, 0), distance + .001)

def fit_front(point):
    side = 1 if point.x > 0 else -1
    envelope = max((distance * math.exp(-((point.z - height) / .026) ** 2)
                    for height, distance in constraints[side].items()), default=0)
    result = point.copy()
    result.x += side * front_weight(point) * envelope
    return result

original_normals = [normal.vector.copy() for normal in hair.data.corner_normals]
normal_transforms = {}
front_displacements = []
for vertex in hair.data.vertices:
    before = vertex.co.copy()
    after = fit_front(before)
    displacement = (after - before).length
    if displacement < 1e-7:
        continue
    columns = []
    for axis in range(3):
        delta = Vector((0, 0, 0))
        delta[axis] = 1e-5
        columns.append((fit_front(before + delta) - fit_front(before - delta)) / 2e-5)
    normal_transforms[vertex.index] = Matrix(columns).inverted()
    vertex.co = after
    front_displacements.append(displacement)
for loop in hair.data.loops:
    transform = normal_transforms.get(loop.vertex_index)
    if transform:
        original_normals[loop.index] = (transform @ original_normals[loop.index]).normalized()
hair.data.normals_split_custom_set(original_normals)
hair.data.update()
mesh = hair.data
reference_faces = [p for p in mesh.polygons
                   if p.normal.x > .55 and p.normal.y < .12 and p.center.x > .045
                   and -.095 < p.center.y < -.01 and 1.58 < p.center.z < 1.75]
normal_tree = KDTree(len(reference_faces))
for index, face in enumerate(reference_faces):
    normal_tree.insert(face.center, index)
normal_tree.balance()
normals = [normal.vector.copy() for normal in mesh.corner_normals]
smoothed_corners = 0

for face in mesh.polygons:
    if face.normal.x < .2 or face.normal.y > .25:
        continue
    for loop in face.loop_indices:
        point = mesh.vertices[mesh.loops[loop].vertex_index].co
        weight = (transition(point.x, .055, .06) * (1 - transition(point.x, .1, .105))
                  * transition(point.y, -.085, -.08) * (1 - transition(point.y, -.025, -.02))
                  * transition(point.z, 1.615, 1.62) * (1 - transition(point.z, 1.71, 1.715)))
        if weight <= 0:
            continue
        direction = Vector((0, 0, 0))
        for _, index, distance in normal_tree.find_n(point, min(16, len(reference_faces))):
            direction += reference_faces[index].normal / (distance + .005) ** 2
        if direction.length_squared:
            normals[loop] = normals[loop].lerp(direction.normalized(), weight).normalized()
            smoothed_corners += 1
mesh.normals_split_custom_set(normals)
mesh.update()
hair.name = 'hair_female_warrior'
hair['part_id'] = hair.name
hair['rig_id'] = 'human_female_01_local_v1'
texture = ROOT / 'assets/modular_human_female_01/hair/tripo_warrior_v1/texture_v1/basecolor-repaired-v2.png'
material = hair.data.materials[0]
shader = next(node for node in material.node_tree.nodes if node.type == 'BSDF_PRINCIPLED')
image_node = shader.inputs['Base Color'].links[0].from_node
image_node.image = bpy.data.images.load(str(texture), check_existing=True)
normal_texture = ROOT / 'assets/modular_human_female_01/hair/tripo_warrior_v1/texture_v1/normal-repaired-v2.png'
normal_node = shader.inputs['Normal'].links[0].from_node
normal_image_node = normal_node.inputs['Color'].links[0].from_node
normal_image_node.image = bpy.data.images.load(str(normal_texture), check_existing=True)
normal_image_node.image.colorspace_settings.name = 'Non-Color'
bpy.ops.object.select_all(action='DESELECT')
hair.select_set(True)
bpy.context.view_layer.objects.active = hair
bpy.ops.export_scene.gltf(filepath=str(OUT / 'hair-fitted-unskinned.glb'), export_format='GLB', use_selection=True, export_tangents=True)
report = {
    'date': '2026-11-02', 'source': str(SOURCE.relative_to(ROOT)),
    'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
    'source_node': 'tripo_part_1', 'body': str(BODY.relative_to(ROOT)),
    'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
    'scale': 1.4, 'translation_blender_m': [0, .012, .452],
    'overall_scale_adjustment': {'factor': 1.15, 'pivot_blender_m': list(head_center)},
    'vertical_scale_adjustment': {'factor': 1.18, 'pivot_blender_m': head_center.z},
    'vertical_adjustment_m': -.051,
    'rear_scalp_fit': {'method': 'Move rear crown vertices outside the base scalp with depth and height falloff.',
                       'moved_vertices': len(rear_moved), 'maximum_displacement_m': max(rear_moved, default=0)},
    'front_body_fit': {'method': 'Sample front-hair triangles against the body side surface and smoothly displace each lock outward by height; transform normals with the inverse deformation Jacobian.',
                       'clearance_target_m': .006, 'constraint_bins': {str(k): len(v) for k, v in constraints.items()},
                       'moved_vertices': len(front_displacements), 'maximum_displacement_m': max(front_displacements, default=0)},
    'side_normals': {'method': 'Blend side corner normals toward nearby outward hair surfaces with boundary falloff.',
                     'smoothed_corners': smoothed_corners},
    'basecolor_texture': {'path': str(texture.relative_to(ROOT)), 'sha256': hashlib.sha256(texture.read_bytes()).hexdigest()},
    'normal_texture': {'path': str(normal_texture.relative_to(ROOT)), 'sha256': hashlib.sha256(normal_texture.read_bytes()).hexdigest()},
    'scalp_fit': {'method': 'Push nearby scalp-region vertices outside the base body using nearest surface normals with height and front falloff.',
                  'moved_vertices': len(moved), 'maximum_displacement_m': max(moved, default=0)},
    'triangles': sum(len(p.vertices) - 2 for p in hair.data.polygons),
    'status': 'Fitting candidate; motion and mixed equipment require review.'}
(ROOT / 'doc/assets/modular-female-warrior-hair-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
scene = bpy.context.scene
scene.render.engine = 'BLENDER_WORKBENCH'
scene.display.shading.light = 'STUDIO'
scene.display.shading.color_type = 'TEXTURE'
scene.display.shading.show_shadows = True
scene.display.shading.show_cavity = True
scene.render.resolution_x, scene.render.resolution_y = 600, 800
scene.render.resolution_percentage = 100
camera_data = bpy.data.cameras.new('ReviewCamera')
camera = bpy.data.objects.new('ReviewCamera', camera_data)
scene.collection.objects.link(camera)
scene.camera = camera
camera_data.type, camera_data.ortho_scale = 'ORTHO', .62
center = Vector((0, 0, 1.57))
for name, offset in [('front', (0, -3, 0)), ('side', (3, 0, 0)), ('back', (0, 3, 0)), ('angle', (3, -5, .1))]:
    camera.location = center + Vector(offset)
    camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.filepath = str(IMAGES / f'hair-fit-{name}.png')
    bpy.ops.render.render(write_still=True)
print(json.dumps(report))
