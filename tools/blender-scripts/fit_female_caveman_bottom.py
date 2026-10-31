import copy
import hashlib
import json
import math
import sys
from pathlib import Path

import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.mesh_components import welded_islands

BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
SOURCE = ROOT / 'assets/items/female_caveman_bottom/tripo_v1/source/model_url.glb'
OUTPUT = SOURCE.parents[1] / 'fitted_v9'
OUTPUT.mkdir(exist_ok=True)


def smooth(x):
    x = np.clip(x, 0, 1)
    return x * x * (3 - 2 * x)


bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.context.preferences.filepaths.save_version = 0
scene = bpy.context.scene
scene.unit_settings.system = 'METRIC'
scene.unit_settings.scale_length = 1
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
rig.animation_data_clear()
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.view_layer.update()
body = bpy.data.objects['female_body']
bp = np.array([body.matrix_world @ vertex.co for vertex in body.data.vertices])
bf = np.array([face.vertices[:] for face in body.data.polygons])
_, _, body_labels = welded_islands(bp, bf)
skin_label = np.bincount(body_labels).argmax()
skin_faces = bf[(body_labels[bf] == skin_label).all(axis=1)]
names = [bone.name for bone in rig.data.bones]
body_weights = np.zeros((len(bp), len(names)))
for vertex in body.data.vertices:
    for group in vertex.groups:
        name = body.vertex_groups[group.group].name
        if name in names:
            body_weights[vertex.index, names.index(name)] = group.weight
arm_groups = [i for i, name in enumerate(names) if 'Arm' in name or 'Hand' in name]
arm_skin = body_weights[:, arm_groups].sum(axis=1) > .35
lower_faces = skin_faces[(bp[skin_faces, 2].mean(axis=1) < 1.16) & ~arm_skin[skin_faces].any(axis=1)]
surface = BVHTree.FromPolygons([Vector(p) for p in bp], lower_faces.tolist())

before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
skirt = next(obj for obj in set(bpy.data.objects) - before if obj.type == 'MESH')
skirt.data.transform(Matrix.Rotation(-math.pi / 2, 4, 'Z') @ skirt.matrix_world)
skirt.matrix_world = Matrix.Identity(4)
mesh = skirt.data
source = np.array([v.co[:] for v in mesh.vertices])
faces = np.array([f.vertices[:] for f in mesh.polygons])
first, welded, labels = welded_islands(source, faces)
shell_label = np.bincount(labels).argmax()
shell_indices = np.flatnonzero(labels == shell_label)
shell_faces = faces[(labels[faces] == shell_label).all(axis=1)]
points = source * np.array([.58, .55, .42]) + np.array([0, .015, .88])
initial = points.copy()

edges = np.unique(np.sort(welded[shell_faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)], axis=1), axis=0)
adjacency = [set() for _ in first]
for a, b in edges:
    if a != b:
        adjacency[a].add(int(b))
        adjacency[b].add(int(a))
for iteration in range(3):
    correction = np.zeros((len(first), 3))
    count = np.zeros(len(first))
    for bary in [(1, 0, 0), (0, 1, 0), (0, 0, 1), (1/3, 1/3, 1/3), (.5, .5, 0), (.5, 0, .5), (0, .5, .5)]:
        samples = np.einsum('fki,k->fi', points[shell_faces], bary)
        for face, point in zip(shell_faces, samples):
            hit, normal, _, _ = surface.find_nearest(Vector(point))
            signed = np.dot(point - np.array(hit), np.array(normal))
            if signed < .004:
                delta = np.array(normal) * min(.004 - signed, .02)
                for vertex in set(welded[face]):
                    correction[vertex] += delta
                    count[vertex] += 1
    correction /= np.maximum(count[:, None], 1)
    updated = correction.copy()
    for i, neighbors in enumerate(adjacency):
        if neighbors:
            updated[i] = .8 * correction[i] + .2 * correction[list(neighbors)].mean(axis=0)
    points[shell_indices] += updated[welded[shell_indices]] * smooth((1.04 - initial[shell_indices, 2]) / .05)[:, None]

shell_tree = KDTree(len(shell_indices))
for i in shell_indices:
    shell_tree.insert(Vector(initial[i]), int(i))
shell_tree.balance()
for label in np.unique(labels):
    if label == shell_label:
        continue
    selected = np.flatnonzero(labels == label)
    anchor = min((shell_tree.find(Vector(initial[i])) for i in selected), key=lambda x: x[2])[1]
    points[selected] += points[anchor] - initial[anchor]

waist = smooth((points[:, 2] - 1.005) / .055)
back = smooth((points[:, 1] + .015) / .12)
points[:, 0] *= 1 - .07 * waist
points[:, 1] = .015 + (points[:, 1] - .015) * (1 - waist * (.035 + .13 * back))
waist_before_clearance = points.copy()
face_points = points[shell_faces]
centers = face_points.mean(axis=1)
face_normals = np.cross(face_points[:, 1] - face_points[:, 0], face_points[:, 2] - face_points[:, 0])
radial = centers.copy()
radial[:, 1] -= .015
radial[:, 2] = 0
outward = np.einsum('ij,ij->i', face_normals, radial) > 0
waist_faces = shell_faces[outward & (centers[:, 2] > 1.025)]
for iteration in range(12):
    correction = np.zeros((len(first), 3))
    count = np.zeros(len(first))
    for bary in [(1, 0, 0), (0, 1, 0), (0, 0, 1), (1/3, 1/3, 1/3), (.5, .5, 0), (.5, 0, .5), (0, .5, .5)]:
        samples = np.einsum('fki,k->fi', points[waist_faces], bary)
        for face, point in zip(waist_faces, samples):
            hit, normal, _, _ = surface.find_nearest(Vector(point))
            signed = np.dot(point - np.array(hit), np.array(normal))
            if signed < .004:
                delta = np.array(normal) * min(.004 - signed, .02)
                for vertex in set(welded[face]):
                    correction[vertex] += delta
                    count[vertex] += 1
    correction /= np.maximum(count[:, None], 1)
    updated = correction.copy()
    for i, neighbors in enumerate(adjacency):
        if neighbors:
            updated[i] = .85 * correction[i] + .15 * correction[list(neighbors)].mean(axis=0)
    points[shell_indices] += updated[welded[shell_indices]]
for label in np.unique(labels):
    if label == shell_label:
        continue
    selected = np.flatnonzero(labels == label)
    anchor = min((shell_tree.find(Vector(initial[i])) for i in selected), key=lambda x: x[2])[1]
    points[selected] += points[anchor] - waist_before_clearance[anchor]

weights = np.zeros((len(points), len(names)))
leg = .5 * smooth((1.015 - points[:, 2]) / .24)
left = smooth((points[:, 0] + .09) / .18)
spine = .08 * smooth((points[:, 2] - 1.02) / .04)
weights[:, names.index('Hips')] = 1 - leg - spine
weights[:, names.index('Spine')] = spine
weights[:, names.index('LeftUpLeg')] = leg * left
weights[:, names.index('RightUpLeg')] = leg * (1 - left)
allowed = [names.index(name) for name in ('Hips', 'Spine', 'LeftUpLeg', 'RightUpLeg')]
for i in shell_indices:
    blend = smooth((points[i, 2] - 1.01) / .04)
    if blend == 0:
        continue
    hit, _, face_index, _ = surface.find_nearest(Vector(points[i]))
    vertices = lower_faces[face_index]
    a, b, c = bp[vertices]
    uv = np.linalg.lstsq(np.column_stack((b - a, c - a)), np.array(hit) - a, rcond=None)[0]
    bary = np.clip([1 - uv.sum(), *uv], 0, 1)
    bary /= bary.sum()
    sampled = bary @ body_weights[vertices]
    sampled[[j for j in range(len(names)) if j not in allowed]] = 0
    if sampled.sum() > 1e-8:
        sampled /= sampled.sum()
        weights[i] = (1 - blend) * weights[i] + blend * sampled
for label in np.unique(labels):
    if label == shell_label:
        continue
    selected = np.flatnonzero(labels == label)
    anchor = min((shell_tree.find(Vector(initial[i])) for i in selected), key=lambda x: x[2])[1]
    weights[selected] = weights[anchor]
order = np.argsort(weights, axis=1)[:, -4:][:, ::-1]
values = np.take_along_axis(weights, order, axis=1)
values[values < .001] = 0
values /= values.sum(axis=1, keepdims=True)
order[values == 0] = 0
for vertex, point in zip(mesh.vertices, points):
    vertex.co = point
mesh.update()
normal = np.cross(points[faces[:, 1]] - points[faces[:, 0]], points[faces[:, 2]] - points[faces[:, 0]])
normal_sum = np.zeros((len(first), 3))
for k in range(3):
    np.add.at(normal_sum, welded[faces[:, k]], normal)
normal_sum /= np.maximum(np.linalg.norm(normal_sum, axis=1, keepdims=True), 1e-12)
mesh.normals_split_custom_set([normal_sum[welded[loop.vertex_index]] for loop in mesh.loops])
for group in list(skirt.vertex_groups):
    skirt.vertex_groups.remove(group)
for name in names:
    skirt.vertex_groups.new(name=name)
for i in range(len(points)):
    for joint, weight in zip(order[i], values[i]):
        if weight:
            skirt.vertex_groups[int(joint)].add([i], float(weight), 'REPLACE')
modifier = skirt.modifiers.new('Female rig', 'ARMATURE')
modifier.object = rig
skirt.parent = rig
skirt.name = mesh.name = 'bottom_female_caveman'
for key, data in [('_RIG_JOINTS', order), ('_RIG_WEIGHTS', values)]:
    mesh.attributes.new(key, 'FLOAT_COLOR', 'POINT').data.foreach_set('color', data.astype(np.float32).ravel())
bpy.ops.object.select_all(action='DESELECT')
skirt.select_set(True)
bpy.context.view_layer.objects.active = skirt
intermediate = OUTPUT / 'geometry.glb'
modifier.show_viewport = modifier.show_render = False
bpy.ops.export_scene.gltf(filepath=str(intermediate), export_format='GLB', use_selection=True,
                          export_animations=False, export_skins=False, export_attributes=True, export_tangents=True)
modifier.show_viewport = modifier.show_render = True

doc, raw = read_glb(intermediate)
raw = bytearray(raw)
base, base_raw = read_glb(BODY)
attrs = doc['meshes'][0]['primitives'][0]['attributes']
source_order = accessor(doc, raw, attrs.pop('_RIG_JOINTS')).astype(np.uint16)
source_values = accessor(doc, raw, attrs.pop('_RIG_WEIGHTS'))
canonical = [base['nodes'][j]['name'] for j in base['skins'][0]['joints']]
remap_order = np.array([canonical.index(name) for name in names])
attrs['JOINTS_0'] = add_accessor(doc, raw, remap_order[source_order], 'VEC4', 5123)
attrs['WEIGHTS_0'] = add_accessor(doc, raw, source_values, 'VEC4')
ids = base['skins'][0]['joints']
remap = {old: new for new, old in enumerate(ids)}
doc['nodes'] = [copy.deepcopy(base['nodes'][i]) for i in ids]
for node in doc['nodes']:
    if 'children' in node:
        node['children'] = [remap[i] for i in node['children'] if i in remap]
children = {i for node in doc['nodes'] for i in node.get('children', [])}
doc['nodes'].append({'name': 'Armature', 'children': [i for i in range(len(ids)) if i not in children]})
doc['nodes'].append({'name': skirt.name, 'mesh': 0, 'skin': 0,
                     'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': skirt.name}})
skin = copy.deepcopy(base['skins'][0])
skin['joints'] = list(range(len(ids)))
if 'skeleton' in skin:
    skin['skeleton'] = remap[skin['skeleton']]
skin['inverseBindMatrices'] = add_accessor(doc, raw, accessor(base, base_raw, base['skins'][0]['inverseBindMatrices']), 'MAT4')
doc['skins'] = [skin]
doc['scenes'] = [{'nodes': [len(ids), len(ids) + 1]}]
doc['scene'] = 0
doc.pop('animations', None)
doc['buffers'] = [{'byteLength': len(raw)}]
write_glb(OUTPUT / 'bottom_female_caveman.glb', doc, raw)
intermediate.unlink()
np.savez(OUTPUT / 'fit-data.npz', source=source, points=points, faces=faces, joints=order, weights=values, bone_names=np.array(names))
bpy.ops.file.pack_all()
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'female-caveman-bottom.blend'))
report = {'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
          'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
          'triangles': len(faces), 'vertices': len(points), 'bones': len(names),
          'scale_blender': [.58, .55, .42], 'translation_blender_m': [0, .015, .88],
          'shell_clearance_iterations': 3, 'leg_weight_maximum': .5,
          'waist_width_contraction': .07, 'waist_depth_contraction': [.035, .165],
          'waist_clearance_iterations': 12, 'waist_body_weight_transfer': True,
          'units': 'metres', 'status': 'Fitting candidate; animation and visual review required'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
