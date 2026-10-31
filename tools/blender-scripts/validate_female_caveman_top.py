import hashlib
import json
import os
import sys
from pathlib import Path

import bpy
import bmesh
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb
from lib.glb_accessors import accessor

BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
DIRECTORY = ROOT / 'assets/items/female_caveman_top/tripo_v1' / os.environ.get('CAVEMAN_FIT', 'fitted_v7')
TOP = DIRECTORY / 'top_female_caveman.glb'

body_doc, body_raw = read_glb(BODY)
top_doc, top_raw = read_glb(TOP)
body_skin, top_skin = body_doc['skins'][0], top_doc['skins'][0]
assert [body_doc['nodes'][i]['name'] for i in body_skin['joints']] == [top_doc['nodes'][i]['name'] for i in top_skin['joints']]
for body_id, top_id in zip(body_skin['joints'], top_skin['joints']):
    original, fitted = body_doc['nodes'][body_id], top_doc['nodes'][top_id]
    for key in ('translation', 'rotation', 'scale', 'matrix'):
        assert original.get(key) == fitted.get(key)
    assert [body_doc['nodes'][i]['name'] for i in original.get('children', []) if i in body_skin['joints']] == [top_doc['nodes'][i]['name'] for i in fitted.get('children', []) if i in top_skin['joints']]
assert np.array_equal(accessor(body_doc, body_raw, body_skin['inverseBindMatrices']), accessor(top_doc, top_raw, top_skin['inverseBindMatrices']))
attrs = top_doc['meshes'][0]['primitives'][0]['attributes']
weights = accessor(top_doc, top_raw, attrs['WEIGHTS_0'])
joints = accessor(top_doc, top_raw, attrs['JOINTS_0'])
assert np.isfinite(weights).all() and np.allclose(weights.sum(axis=1), 1, atol=1e-6)
assert (joints < len(top_skin['joints'])).all()

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
body = bpy.data.objects['female_body']
base_points = np.array([body.matrix_world @ vertex.co for vertex in body.data.vertices])
base_faces = np.array([face.vertices[:] for face in body.data.polygons])
_, inverse = np.unique(np.round(base_points, 6), axis=0, return_inverse=True)
parent = np.arange(inverse.max() + 1)


def root(i):
    while parent[i] != i:
        parent[i] = parent[parent[i]]
        i = parent[i]
    return i


for face in base_faces:
    first = root(inverse[face[0]])
    for vertex in face[1:]:
        parent[root(inverse[vertex])] = first
labels = np.array([root(i) for i in inverse])
skin_label = np.bincount(labels).argmax()
skin_faces = base_faces[(labels[base_faces] == skin_label).all(1)]
arm_groups = {group.index for group in body.vertex_groups if 'Arm' in group.name or 'Hand' in group.name}
arm_skin = np.array([sum(group.weight for group in vertex.groups if group.group in arm_groups) > .35 for vertex in body.data.vertices])
torso_faces = skin_faces[~arm_skin[skin_faces].any(axis=1)]
underwear = np.zeros(len(base_faces), dtype=bool)
for label in np.unique(labels):
    q = base_points[labels == label]
    if label != skin_label and q[:, 2].min() > 1.15 and q[:, 2].max() < 1.55:
        underwear |= (labels[base_faces] == label).all(1)
visual_body = body.copy()
visual_body.data = body.data.copy()
bpy.context.collection.objects.link(visual_body)
edit = bmesh.new()
edit.from_mesh(visual_body.data)
edit.faces.ensure_lookup_table()
bmesh.ops.delete(edit, geom=[edit.faces[i] for i in np.flatnonzero(underwear)], context='FACES')
edit.to_mesh(visual_body.data)
edit.free()
body.hide_render = True
for track in rig.animation_data.nla_tracks:
    track.mute = True
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(TOP), bone_heuristic='TEMPERANCE')
top = next(obj for obj in set(bpy.data.objects) - before if obj.type == 'MESH')
original_matrix = top.matrix_world.copy()
top.parent = rig
top.matrix_world = original_matrix
for modifier in top.modifiers:
    if modifier.type == 'ARMATURE':
        modifier.object = rig
for name in ['idle1', 'idle_armed', 'walk', 'run', 'jump', 'slash1', 'sit_idle', 'combat_idle', 'dying']:
    assert bpy.data.actions.get(name), name

scene = bpy.context.scene
scene.render.engine = 'BLENDER_WORKBENCH'
scene.display.shading.color_type = 'TEXTURE'
scene.display.shading.light = 'STUDIO'
scene.render.resolution_x = scene.render.resolution_y = 800
bpy.ops.object.camera_add()
camera = bpy.context.object
camera.data.type = 'ORTHO'
camera.data.ortho_scale = .8
scene.camera = camera
camera.location = (0, -3, 1.34)
camera.rotation_euler = (Vector((0, 0, 1.34)) - camera.location).to_track_quat('-Z', 'Y').to_euler()
(DIRECTORY / 'animation-review').mkdir(exist_ok=True)

fit = np.load(DIRECTORY / 'fit-data.npz')
rest_points, faces = fit['points'], fit['faces']
_, welded = np.unique(np.round(fit['source'], 6), axis=0, return_inverse=True)
part_parent = np.arange(welded.max() + 1)


def part_root(i):
    while part_parent[i] != i:
        part_parent[i] = part_parent[part_parent[i]]
        i = part_parent[i]
    return i


for face in faces:
    first = part_root(welded[face[0]])
    for vertex in face[1:]:
        part_parent[part_root(welded[vertex])] = first
part_labels = np.array([part_root(i) for i in welded])
shell_label = np.bincount(part_labels).argmax()
shell_faces = faces[(part_labels[faces] == shell_label).all(1)]
centers = rest_points[shell_faces].mean(axis=1)
cup_faces = shell_faces[(centers[:, 1] < -.045) & (np.abs(centers[:, 0]) > .025)
                        & (centers[:, 2] > 1.28) & (centers[:, 2] < 1.48)]
barycentric = np.array([(a / 4, b / 4, (4 - a - b) / 4)
                        for a in range(5) for b in range(5 - a)])
fitting = json.loads((DIRECTORY / 'fitting.json').read_text())
fang_roots = {}
rear_tips = {}
pendant_tips = {}
for fang in fitting['fangs']:
    vertices = np.flatnonzero(part_labels == fang['label'])
    threshold = np.quantile(rest_points[vertices, 2], .9)
    fang_roots[fang['label']] = vertices[rest_points[vertices, 2] >= threshold]
    if fang['type'] == 'rear':
        threshold = np.quantile(rest_points[vertices, 2], .1)
        rear_tips[fang['label']] = vertices[rest_points[vertices, 2] <= threshold]
    if fang['type'] == 'pendant':
        threshold = np.quantile(rest_points[vertices, 2], .1)
        pendant_tips[fang['label']] = vertices[rest_points[vertices, 2] <= threshold]
edges = np.unique(np.sort(faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2), axis=1), axis=0)
rest_lengths = np.linalg.norm(rest_points[edges[:, 0]] - rest_points[edges[:, 1]], axis=1)
edges = edges[rest_lengths > .005]
rest_lengths = rest_lengths[rest_lengths > .005]
reports = []
for name in ['idle1', 'idle_armed', 'walk', 'run', 'jump', 'slash1', 'sit_idle', 'combat_idle', 'dying']:
    action = bpy.data.actions[name]
    rig.animation_data.action = action
    worst_penetration = 0.0
    worst_count = 0
    worst_stretch = 0.0
    worst_cup_count = 0
    worst_cup_depth = 0.0
    worst_region = None
    worst_torso = 0
    worst_torso_depth = 0.0
    fang_gaps = {label: 0.0 for label in fang_roots}
    rear_tip_gaps = {label: 0.0 for label in rear_tips}
    pendant_tucked = {label: 0 for label in pendant_tips}
    pendant_tip_gaps = {label: 0.0 for label in pendant_tips}
    for sample, frame in enumerate(np.linspace(*action.frame_range, 13)):
        scene.frame_set(int(frame), subframe=float(frame % 1))
        bpy.context.view_layer.update()
        depsgraph = bpy.context.evaluated_depsgraph_get()
        posed_top = top.evaluated_get(depsgraph)
        posed_body = body.evaluated_get(depsgraph)
        top_mesh = posed_top.to_mesh()
        body_mesh = posed_body.to_mesh()
        points = np.array([posed_top.matrix_world @ vertex.co for vertex in top_mesh.vertices])
        body_points = np.array([posed_body.matrix_world @ vertex.co for vertex in body_mesh.vertices])
        assert len(points) == len(rest_points) and np.isfinite(points).all()
        tree = BVHTree.FromPolygons([Vector(p) for p in body_points], skin_faces.tolist())
        torso_tree = BVHTree.FromPolygons([Vector(p) for p in body_points], torso_faces.tolist())
        cup_points = np.einsum('kj,fji->fki', barycentric, points[cup_faces]).reshape(-1, 3)
        cup_depths = []
        for point in cup_points:
            hit, normal, _, _ = torso_tree.find_nearest(Vector(point))
            cup_depths.append(np.dot(point - np.array(hit), np.array(normal)))
        cup_depths = np.array(cup_depths)
        worst_cup_count = max(worst_cup_count, int((cup_depths < -.001).sum()))
        worst_cup_depth = max(worst_cup_depth, float(max(0, -cup_depths.min())))
        shell_tree = BVHTree.FromPolygons([Vector(p) for p in points], shell_faces.tolist())
        for label, vertices in fang_roots.items():
            root_center = points[vertices].mean(axis=0)
            distance = shell_tree.find_nearest(Vector(root_center))[3]
            fang_gaps[label] = max(fang_gaps[label], float(distance))
        for label, vertices in rear_tips.items():
            tip_center = points[vertices].mean(axis=0)
            distance = shell_tree.find_nearest(Vector(tip_center))[3]
            rear_tip_gaps[label] = max(rear_tip_gaps[label], float(distance))
        spine = rig.pose.bones['Spine1']
        rotation = rig.matrix_world.to_3x3() @ spine.matrix.to_3x3() @ spine.bone.matrix_local.to_3x3().inverted()
        forward = (rotation @ Vector((0, -1, 0))).normalized()
        for label, vertices in pendant_tips.items():
            root_center = points[fang_roots[label]].mean(axis=0)
            if shell_tree.ray_cast(Vector(root_center), forward, .04)[0] is not None:
                pendant_tucked[label] += 1
            tip_center = points[vertices].mean(axis=0)
            distance = tree.find_nearest(Vector(tip_center))[3]
            pendant_tip_gaps[label] = max(pendant_tip_gaps[label], float(distance))
        depths = []
        torso_depths = []
        for point in points:
            hit, normal, _, _ = tree.find_nearest(Vector(point))
            depths.append(np.dot(point - np.array(hit), np.array(normal)))
            hit, normal, _, _ = torso_tree.find_nearest(Vector(point))
            torso_depths.append(np.dot(point - np.array(hit), np.array(normal)))
        depths = np.array(depths)
        torso_depths = np.array(torso_depths)
        worst_torso = max(worst_torso, int((torso_depths < -.003).sum()))
        worst_torso_depth = max(worst_torso_depth, float(max(0, -torso_depths.min())))
        worst_penetration = max(worst_penetration, float(max(0, -depths.min())))
        count = int((depths < -.003).sum())
        if count > worst_count:
            worst_count = count
            selected = points[depths < -.003]
            worst_region = {'frame': float(frame), 'min': selected.min(0).tolist(), 'max': selected.max(0).tolist()}
        lengths = np.linalg.norm(points[edges[:, 0]] - points[edges[:, 1]], axis=1)
        worst_stretch = max(worst_stretch, float((lengths / rest_lengths).max()))
        posed_top.to_mesh_clear()
        posed_body.to_mesh_clear()
        if not os.environ.get('CAVEMAN_NO_RENDER') and sample == 6 and name in ('idle1', 'walk', 'jump', 'slash1', 'sit_idle'):
            scene.render.filepath = str(DIRECTORY / 'animation-review' / f'{name}.png')
            bpy.ops.render.render(write_still=True)
    if not os.environ.get('CAVEMAN_NO_RENDER') and name == 'walk' and worst_region:
        frame = worst_region['frame']
        scene.frame_set(int(frame), subframe=float(frame % 1))
        scene.render.filepath = str(DIRECTORY / 'animation-review' / 'walk-worst.png')
        bpy.ops.render.render(write_still=True)
    reports.append({'clip': name, 'samples': 13, 'penetrating_vertices_over_3mm': worst_count,
                    'maximum_penetration_m': worst_penetration, 'cup_samples_over_1mm': worst_cup_count,
                    'maximum_cup_penetration_m': worst_cup_depth, 'torso_vertices_over_3mm': worst_torso,
                    'maximum_torso_penetration_m': worst_torso_depth, 'maximum_edge_ratio': worst_stretch,
                    'maximum_fang_root_gaps_m': fang_gaps, 'maximum_rear_tip_gaps_m': rear_tip_gaps,
                    'pendant_root_tucked_samples': pendant_tucked, 'maximum_pendant_tip_skin_gaps_m': pendant_tip_gaps,
                    'worst_region': worst_region})
    print(name, reports[-1], flush=True)
report = {'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
          'top_sha256': hashlib.sha256(TOP.read_bytes()).hexdigest(), 'bones': len(top_skin['joints']),
          'exact_hierarchy_and_inverse_bind': True, 'weights_normalized': True, 'clips': reports,
          'cup_faces': len(cup_faces), 'cup_samples_per_face': len(barycentric),
          'scope': '13 evenly spaced poses per clip; cup triangle samples and vertex-to-nearest-skin distances are approximate.'}
(DIRECTORY / 'animation-validation.json').write_text(json.dumps(report, indent=2) + '\n')
