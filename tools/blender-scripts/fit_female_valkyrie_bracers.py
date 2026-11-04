import hashlib
import json
import sys
from pathlib import Path

import bpy
import bmesh
from mathutils import Vector, Matrix

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/items/valkyrie_gloves/source/model.glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUT = ROOT / 'assets/items/valkyrie_gloves/fitted_v2'
REMOVED_FACES = ROOT / 'doc/assets/valkyrie-bracer-skin-faces-v1.json'
OUT.mkdir(parents=True, exist_ok=True)
if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != '686212b78d9410619bbd9cf1a52e31bc7f27deb45654e8300bf8b9ecc19c2993':
    raise ValueError('Unexpected source model')

bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete(use_global=False)
bpy.ops.import_scene.gltf(filepath=str(BODY))
armature = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
armature['rig_id'] = 'human_female_01_local_v1'
body_objects = set(bpy.context.scene.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
source = bpy.data.objects['tripo_part_8']
mesh = source.data.copy()
left = bpy.data.objects.new('valkyrie_bracer_left', mesh)
bpy.context.collection.objects.link(left)
mesh.transform(source.matrix_world)

elbow = armature.matrix_world @ armature.data.bones['LeftForeArm'].head_local
wrist = armature.matrix_world @ armature.data.bones['LeftHand'].head_local
source_wrist = Vector((0.205, -0.0175, 0.54))
source_axis = Vector((0.052, 0, -0.135)).normalized()
target_axis = (wrist - elbow).normalized()
rotation = source_axis.rotation_difference(target_axis).to_matrix().to_4x4()
transform = Matrix.Translation(wrist - target_axis * 0.012) @ rotation @ Matrix.Scale(1.65, 4) @ Matrix.Translation(-source_wrist)
mesh.transform(transform)

for o in list(bpy.context.scene.objects):
    if o not in body_objects and o != left:
        bpy.data.objects.remove(o, do_unlink=True)

right = left.copy()
right.data = left.data.copy()
right.name = 'valkyrie_bracer_right'
bpy.context.collection.objects.link(right)
right.data.transform(Matrix.Diagonal((-1, 1, 1, 1)))
right.data.flip_normals()
right_wrist = armature.matrix_world @ armature.data.bones['RightHand'].head_local
for vertex in right.data.vertices:
    vertex.co.y += right_wrist.y - wrist.y

removed_faces = json.loads(REMOVED_FACES.read_text())
if len(left.data.polygons) != 891 or any(i < 0 or i >= 891 for i in removed_faces):
    raise ValueError('Unexpected source faces')
for obj, side in ((left, 'Left'), (right, 'Right')):
    corners = [Vector((v.co.x, v.co.z, -v.co.y)) for v in obj.data.vertices]
    obj['fit_center'] = [(min(v[i] for v in corners) + max(v[i] for v in corners)) / 2 for i in range(3)]
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.faces.ensure_lookup_table()
    bmesh.ops.delete(bm, geom=[bm.faces[i] for i in removed_faces], context='FACES')
    bm.to_mesh(obj.data)
    bm.free()
    group = obj.vertex_groups.new(name=side + 'ForeArm')
    group.add(list(range(len(obj.data.vertices))), 1.0, 'REPLACE')
    obj.parent = armature
    mod = obj.modifiers.new('Armature', 'ARMATURE')
    mod.object = armature

bpy.context.scene.unit_settings.system = 'METRIC'
bpy.context.scene.unit_settings.scale_length = 1
if '--blend' in sys.argv:
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT / 'fitting.blend'))
bpy.ops.object.select_all(action='DESELECT')
for obj in (armature, left, right):
    obj.select_set(True)
bpy.context.view_layer.objects.active = armature
bpy.ops.export_scene.gltf(filepath=str(OUT / 'bracers_female_valkyrie.glb'), export_format='GLB', use_selection=True, export_animations=False, export_extras=True)
report = {
    'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
    'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
    'source_object': 'tripo_part_8',
    'removed_source_faces_per_side': len(removed_faces),
    'removed_face_indices': str(REMOVED_FACES.relative_to(ROOT)),
    'vertices_per_side': len(left.data.vertices),
    'triangles_per_side': sum(len(p.vertices) - 2 for p in left.data.polygons),
    'weights': {'left': 'LeftForeArm: 1.0', 'right': 'RightForeArm: 1.0'},
    'status': 'initial fitting; visual, clearance, animation and mixed-outfit review pending',
}
(OUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
