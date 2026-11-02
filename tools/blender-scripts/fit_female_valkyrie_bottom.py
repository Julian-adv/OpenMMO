import copy
import hashlib
import json
import sys
from pathlib import Path

import bpy
import numpy as np
from mathutils import Matrix
from mathutils.kdtree import KDTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.mesh_components import welded_islands

BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
SOURCE = ROOT / 'assets/items/valkyrie_starter/tripo_bottom_v1/source.glb'
VERSION = sys.argv[sys.argv.index('--') + 1] if '--' in sys.argv else 'v4'
if VERSION not in ('v1', 'v2', 'v3', 'v4'):
    raise ValueError(f'Unknown fitting version: {VERSION}')
OUTPUT = SOURCE.parent / f'fitted_{VERSION}'
OUTPUT.mkdir(exist_ok=True)


def smooth(value):
    value = np.clip(value, 0, 1)
    return value * value * (3 - 2 * value)


bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.context.preferences.filepaths.save_version = 0
bpy.context.scene.unit_settings.system = 'METRIC'
bpy.context.scene.unit_settings.scale_length = 1
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
rig.animation_data_clear()
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.view_layer.update()
names = [bone.name for bone in rig.data.bones]

before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
part = next(obj for obj in set(bpy.data.objects) - before if obj.type == 'MESH')
part.data.transform(part.matrix_world)
part.matrix_world = Matrix.Identity(4)
mesh = part.data
source = np.array([vertex.co[:] for vertex in mesh.vertices])
faces = np.array([face.vertices[:] for face in mesh.polygons])
first, welded, labels = welded_islands(source, faces)
scale = np.array([.49, .55, .39])
counts = np.bincount(labels)
shells = np.flatnonzero(counts >= 200)
main = np.isin(labels, shells)
side_labels = [label for label in shells if counts[label] >= 500]
panel_side = np.zeros(len(source))
for label in side_labels:
    panel_side[labels == label] = np.sign(source[labels == label, 0].mean())
source_tree = KDTree(int(main.sum()))
for i in np.flatnonzero(main):
    source_tree.insert(source[i], int(i))
source_tree.balance()
for label in np.unique(labels[~main]):
    selected = np.flatnonzero(labels == label)
    _, anchor, _ = source_tree.find(source[selected].mean(axis=0))
    panel_side[selected] = panel_side[anchor]
panel_side = panel_side[first][welded]


def fit_shape(coords, cover=True):
    fitted = coords * scale + np.array([0, 0, .85])
    if VERSION in ('v2', 'v3', 'v4'):
        front = smooth((-fitted[:, 1] - .005) / .045)
        center = smooth((.21 - np.abs(fitted[:, 0])) / .06)
        height = smooth((fitted[:, 2] - .97) / .05)
        fitted[:, 1] -= .03 * front * center * height
    if VERSION in ('v3', 'v4'):
        waist = smooth((fitted[:, 2] - 1.02) / .075)
        rear = smooth((fitted[:, 1] + .075) / .08)
        amount = waist * rear * np.abs(panel_side)
        fitted[:, 0] -= .018 * panel_side * amount
        fitted[:, 1] -= .29 * np.maximum(fitted[:, 1], 0) * amount
        fitted[:, 2] -= .012 * amount
    if VERSION == 'v4' and cover:
        central = 1 - np.abs(panel_side)
        front = smooth((-fitted[:, 1] - .03) / .045)
        height = smooth((fitted[:, 2] - .99) / .03) * (1 - smooth((fitted[:, 2] - 1.06) / .05))
        inner = 1 - smooth((np.abs(fitted[:, 0]) - .025) / .04)
        outer = smooth((np.abs(fitted[:, 0]) - .06) / .04) * (1 - smooth((np.abs(fitted[:, 0]) - .14) / .05))
        adjustment = np.zeros_like(fitted)
        adjustment[:, 0] = .020 * np.sign(fitted[:, 0]) * central * front * height * smooth((np.abs(fitted[:, 0]) - .075) / .04)
        adjustment[:, 2] = central * front * height * (.022 * inner - .025 * outer)
        for label in np.unique(labels[~main]):
            selected = labels == label
            adjustment[selected] = adjustment[selected].mean(axis=0)
        fitted += adjustment
    return fitted


points = fit_shape(source)
epsilon = 1e-5
derivatives = []
base_derivatives = []
for axis in range(3):
    offset = np.zeros(3)
    offset[axis] = epsilon
    derivatives.append((fit_shape(source + offset) - fit_shape(source - offset)) / (2 * epsilon))
    if VERSION == 'v4':
        base_derivatives.append((fit_shape(source + offset, cover=False) - fit_shape(source - offset, cover=False)) / (2 * epsilon))
loop_vertices = np.array([loop.vertex_index for loop in mesh.loops])
source_normals = np.array([normal.vector[:] for normal in mesh.corner_normals])
jacobians = np.stack(derivatives, axis=2)
if VERSION == 'v4':
    jacobians[~main] = np.stack(base_derivatives, axis=2)[~main]
minimum_determinant = float(np.linalg.det(jacobians).min())
if minimum_determinant <= 0:
    raise ValueError('Fitting deformation folds the mesh')
normals = np.linalg.solve(jacobians[loop_vertices].transpose(0, 2, 1), source_normals[:, :, None])[:, :, 0]
normals /= np.maximum(np.linalg.norm(normals, axis=1, keepdims=True), 1e-12)

weights = np.zeros((len(points), len(names)))
leg = .24 * smooth((np.abs(points[:, 0]) - .06) / .10) * smooth((1.16 - points[:, 2]) / .20)
weights[:, names.index('Hips')] = 1 - leg
weights[:, names.index('LeftUpLeg')] = leg * (points[:, 0] > 0)
weights[:, names.index('RightUpLeg')] = leg * (points[:, 0] < 0)

shell_tree = KDTree(int(main.sum()))
for i in np.flatnonzero(main):
    shell_tree.insert(points[i], int(i))
shell_tree.balance()
for label in np.unique(labels[~main]):
    selected = np.flatnonzero(labels == label)
    center = points[selected].mean(axis=0)
    _, anchor, _ = shell_tree.find(center)
    weights[selected] = weights[anchor]
weights = weights[first][welded]
order = np.argsort(weights, axis=1)[:, -4:][:, ::-1]
values = np.take_along_axis(weights, order, axis=1)
values /= values.sum(axis=1, keepdims=True)
order[values == 0] = 0

for vertex, point in zip(mesh.vertices, points):
    vertex.co = point
mesh.update()
mesh.normals_split_custom_set(normals.tolist())
for group in list(part.vertex_groups):
    part.vertex_groups.remove(group)
for name in names:
    part.vertex_groups.new(name=name)
for i in range(len(points)):
    for joint, weight in zip(order[i], values[i]):
        if weight:
            part.vertex_groups[int(joint)].add([i], float(weight), 'REPLACE')
modifier = part.modifiers.new('Female rig', 'ARMATURE')
modifier.object = rig
part.parent = rig
part.name = mesh.name = 'bottom_female_valkyrie'
for key, data in [('_RIG_JOINTS', order), ('_RIG_WEIGHTS', values)]:
    mesh.attributes.new(key, 'FLOAT_COLOR', 'POINT').data.foreach_set('color', data.astype(np.float32).ravel())
bpy.ops.object.select_all(action='DESELECT')
part.select_set(True)
bpy.context.view_layer.objects.active = part
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
doc['nodes'].append({'name': part.name, 'mesh': 0, 'skin': 0,
                     'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': part.name}})
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
output = OUTPUT / 'bottom_female_valkyrie.glb'
write_glb(output, doc, raw)
intermediate.unlink()
bpy.ops.file.pack_all()
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'fitting.blend'))
body_primitive = base['meshes'][0]['primitives'][0]
mask = {'body': str(BODY.relative_to(ROOT)), 'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
        'bottom': part.name, 'bottom_sha256': hashlib.sha256(output.read_bytes()).hexdigest(),
        'mesh_index': 0, 'primitive_index': 0,
        'primitive_triangles': len(accessor(base, base_raw, body_primitive['indices'])) // 3,
        'hide_triangle_indices': [], 'scope': 'Preserve body and underwear during initial fitting review.'}
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')
report = {'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
          'body_sha256': mask['body_sha256'], 'triangles': len(faces), 'vertices': len(points),
          'scale_blender': [.49, .55, .39], 'translation_blender_m': [0, 0, .85],
          'front_center_offset_max_m': .03 if VERSION in ('v2', 'v3', 'v4') else 0,
          'upper_side_inward_max_m': .018 if VERSION in ('v3', 'v4') else 0,
          'upper_side_rear_depth_reduction': .29 if VERSION in ('v3', 'v4') else 0,
          'upper_side_lowering_max_m': .012 if VERSION in ('v3', 'v4') else 0,
          'front_center_rim_raise_max_m': .022 if VERSION == 'v4' else 0,
          'front_leg_rim_lowering_max_m': .025 if VERSION == 'v4' else 0,
          'front_leg_rim_outward_max_m': .020 if VERSION == 'v4' else 0,
          'minimum_deformation_jacobian_determinant': minimum_determinant,
          'maximum_leg_weight': .24, 'status': 'Fitting and rig candidate; motion and visual review required.'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
