import copy
import hashlib
import json
import math
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor

DIRECTORY = ROOT / 'assets/items/female_caveman_top/tripo_v1'
SOURCE = DIRECTORY / 'source/model_url.glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUTPUT = DIRECTORY / 'fitted_v7'
OUTPUT.mkdir(exist_ok=True)


def islands(points, faces):
    _, inverse = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    inverse = inverse.ravel()
    parent = np.arange(inverse.max() + 1)

    def find(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for face in faces:
        root = find(inverse[face[0]])
        for vertex in face[1:]:
            parent[find(inverse[vertex])] = root
    return np.array([find(i) for i in inverse])


bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.unit_settings.system = 'METRIC'
scene.unit_settings.scale_length = 1
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(obj for obj in bpy.data.objects if obj.type == 'ARMATURE')
rig.animation_data_clear()
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.view_layer.update()
body = bpy.data.objects['female_body']
bp = np.array([body.matrix_world @ vertex.co for vertex in body.data.vertices])
bf = np.array([face.vertices[:] for face in body.data.polygons])
labels = islands(bp, bf)
skin_label = np.bincount(labels).argmax()
skin_faces = bf[(labels[bf] == skin_label).all(1)]
surface = BVHTree.FromPolygons([Vector(point) for point in bp], skin_faces.tolist())
names = [bone.name for bone in rig.data.bones]
index = {name: i for i, name in enumerate(names)}
body_weights = np.zeros((len(bp), len(names)))
for vertex in body.data.vertices:
    for group in vertex.groups:
        name = body.vertex_groups[group.group].name
        if name in index:
            body_weights[vertex.index, index[name]] = group.weight

before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
top = next(obj for obj in set(bpy.data.objects) - before if obj.type == 'MESH')
transform = Matrix.Translation((0, 0, 1.35)) @ Matrix.Rotation(-math.pi / 2, 4, 'Z') @ Matrix.Scale(.31, 4)
top.data.transform(transform @ top.matrix_world)
top.matrix_world = Matrix.Identity(4)
mesh = top.data
source = np.array([vertex.co for vertex in mesh.vertices])
faces = np.array([face.vertices[:] for face in mesh.polygons])
points = source.copy()
points[:, 1] *= 1.22
points[:, 2] = 1.35 + (points[:, 2] - 1.35) * 1.2
source_labels = islands(source, faces)
for label in np.unique(source_labels):
    selected = source_labels == label
    part = source[selected]
    if (len(part) >= 40 and abs(np.median(part[:, 0])) < .04 and np.median(part[:, 1]) > .07
            and part[:, 2].min() < 1.27 and part[:, 2].max() > 1.32):
        points[selected, 1] -= .26

shell_label = np.bincount(source_labels).argmax()
back_blend = np.clip((points[:, 1] + .01) / .09, 0, 1)
back_blend = back_blend * back_blend * (3 - 2 * back_blend)
for label in np.unique(source_labels):
    if label != shell_label:
        selected = source_labels == label
        back_blend[selected] = back_blend[selected].mean()
points[:, 2] -= .05 * back_blend

_, first, welded = np.unique(np.round(source, 7), axis=0, return_index=True, return_inverse=True)
welded = welded.ravel()
edges = np.unique(np.sort(welded[faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)], axis=1), axis=0)
adjacency = [set() for _ in first]
for a, b in edges:
    if a != b:
        adjacency[a].add(int(b))
        adjacency[b].add(int(a))

for iteration in range(8):
    correction = np.zeros((len(first), 3))
    counts = np.zeros(len(first))
    for vertices, point in [(np.array([i]), point) for i, point in enumerate(points)] + [(face, points[face].mean(0)) for face in faces]:
        hit, normal, _, _ = surface.find_nearest(Vector(point))
        distance = np.dot(point - np.array(hit), np.array(normal))
        if distance < .005:
            delta = np.array(normal) * min(.005 - distance, .025)
            for vertex in set(welded[vertices]):
                correction[vertex] += delta
                counts[vertex] += 1
    correction /= np.maximum(counts[:, None], 1)
    for _ in range(2):
        updated = correction.copy()
        for i, neighbors in enumerate(adjacency):
            if neighbors:
                updated[i] = .65 * correction[i] + .35 * correction[list(neighbors)].mean(0)
        correction = updated
    points += correction[welded]

shell_indices = np.flatnonzero(source_labels == shell_label)
shell_tree = KDTree(len(shell_indices))
for i in shell_indices:
    shell_tree.insert(Vector(points[i]), int(i))
shell_tree.balance()
shell_displacement = np.zeros_like(points)
for i in shell_indices:
    hit, normal, _, _ = surface.find_nearest(Vector(points[i]))
    distance = np.dot(points[i] - np.array(hit), np.array(normal))
    if distance > .006:
        shell_displacement[i] = -np.array(normal) * min(distance - .006, .04) * .9
for label in np.unique(source_labels):
    if label == shell_label:
        continue
    selected = np.flatnonzero(source_labels == label)
    neighbors = [shell_tree.find(Vector(points[i])) for i in selected]
    anchor = min(neighbors, key=lambda value: value[2])[1]
    shell_displacement[selected] = shell_displacement[anchor]
points += shell_displacement

shell_faces = faces[(source_labels[faces] == shell_label).all(axis=1)]
shell_surface = BVHTree.FromPolygons([Vector(point) for point in points], shell_faces.tolist())
fangs = []
for label in np.unique(source_labels):
    selected = source_labels == label
    original = source[selected]
    current = points[selected]
    if len(original) < 40:
        continue
    center = np.median(original, axis=0)
    upper = (.06 < abs(center[0]) < .11 and center[1] < -.075
             and np.ptp(original[:, 2]) > .07 and original[:, 2].max() > 1.42)
    pendant = (abs(center[0]) < .04 and center[1] > .07
               and np.median(current[:, 1]) < -.08 and np.ptp(original[:, 2]) > .08
               and original[:, 2].min() < 1.27 and original[:, 2].max() > 1.32)
    rear = (.11 < abs(center[0]) < .16 and center[1] > .04
            and np.ptp(original[:, 2]) > .07)
    if not (upper or pendant or rear):
        continue
    low, high = np.quantile(current[:, 2], [.1, .9])
    tip = current[current[:, 2] <= low].mean(axis=0)
    root = current[current[:, 2] >= high].mean(axis=0)

    def surface_target(tree, reference):
        hit, normal, _, _ = tree.find_nearest(Vector(reference))
        hit, normal = np.array(hit), np.array(normal)
        if np.dot(reference - hit, normal) < 0:
            normal = -normal
        return hit + normal * .003

    root_target = surface_target(shell_surface, root)
    tip_reference = tip + np.array([0, 0, -.012]) if upper else tip
    tip_target = surface_target(surface if pendant else shell_surface, tip_reference)
    initial_axis = root - tip
    target_axis = root_target - tip_target
    initial_length = np.linalg.norm(initial_axis)
    target_length = np.linalg.norm(target_axis)
    axis = initial_axis / initial_length
    rotation = np.array(Vector(initial_axis).rotation_difference(Vector(target_axis)).to_matrix())
    local = current - tip
    along = local @ axis
    perpendicular = local - along[:, None] * axis
    length_ratio = np.clip(target_length / initial_length, .75, 1.1)
    points[selected] = tip_target + (rotation @ (perpendicular + along[:, None] * axis * length_ratio).T).T
    fang_type = 'upper' if upper else 'pendant' if pendant else 'rear'
    fangs.append({'label': int(label), 'type': fang_type, 'length_ratio': float(length_ratio)})
assert sum(fang['type'] == 'upper' for fang in fangs) == 2
assert sum(fang['type'] == 'pendant' for fang in fangs) == 2
assert sum(fang['type'] == 'rear' for fang in fangs) == 2

weights = np.zeros((len(points), len(names)))
for i, point in enumerate(points):
    hit, _, face_index, _ = surface.find_nearest(Vector(point))
    vertices = skin_faces[face_index]
    a, b, c = bp[vertices]
    uv = np.linalg.lstsq(np.column_stack((b - a, c - a)), np.array(hit) - a, rcond=None)[0]
    bary = np.clip([1 - uv.sum(), *uv], 0, 1)
    bary /= bary.sum()
    weights[i] = bary @ body_weights[vertices]

unique_weights = weights[first].copy()
for _ in range(12):
    updated = unique_weights.copy()
    for i, neighbors in enumerate(adjacency):
        if neighbors:
            updated[i] = .5 * unique_weights[i] + .5 * unique_weights[list(neighbors)].mean(0)
    unique_weights = updated
weights = unique_weights[welded]
part_labels = islands(source, faces)
for label in np.unique(part_labels):
    selected = part_labels == label
    if len(np.unique(welded[selected])) < 120:
        weights[selected] = weights[selected].mean(0)
for side, sign in [('Left', -1), ('Right', 1)]:
    opposite = [j for j, name in enumerate(names) if name.startswith(side) and ('Shoulder' in name or 'Arm' in name)]
    weights[np.ix_(points[:, 0] * sign > .005, opposite)] = 0
for fang in fangs:
    selected = source_labels == fang['label']
    current = points[selected]
    root = current[current[:, 2] >= np.quantile(current[:, 2], .9)].mean(axis=0)

    def shell_weights(point):
        hit, _, face_index, _ = shell_surface.find_nearest(Vector(point))
        vertices = shell_faces[face_index]
        a, b, c = points[vertices]
        uv = np.linalg.lstsq(np.column_stack((b - a, c - a)), np.array(hit) - a, rcond=None)[0]
        bary = np.clip([1 - uv.sum(), *uv], 0, 1)
        bary /= bary.sum()
        return bary @ weights[vertices]

    root_weights = shell_weights(root)
    if fang['type'] == 'rear':
        tip = current[current[:, 2] <= np.quantile(current[:, 2], .1)].mean(axis=0)
        tip_weights = shell_weights(tip)
        blend = np.clip((root[2] - current[:, 2]) / max(root[2] - tip[2], 1e-8), 0, 1)
        blend = blend * blend * (3 - 2 * blend)
        weights[selected] = (1 - blend[:, None]) * root_weights + blend[:, None] * tip_weights
    else:
        weights[selected] = root_weights
order = np.argsort(weights, axis=1)[:, -4:][:, ::-1]
values = np.take_along_axis(weights, order, axis=1)
values[values < .005] = 0
values /= values.sum(1, keepdims=True)
order[values == 0] = 0

for vertex, point in zip(mesh.vertices, points):
    vertex.co = point
mesh.update()
for group in list(top.vertex_groups):
    top.vertex_groups.remove(group)
for name in names:
    top.vertex_groups.new(name=name)
for i in range(len(points)):
    for joint, weight in zip(order[i], values[i]):
        if weight:
            top.vertex_groups[int(joint)].add([i], float(weight), 'REPLACE')
modifier = top.modifiers.new('Female rig', 'ARMATURE')
modifier.object = rig
top.parent = rig
top.name = mesh.name = 'top_female_caveman'
if '--animation-clearance' in sys.argv:
    rig.animation_data_create()
    assert np.allclose(np.array(top.matrix_world), np.eye(4), atol=1e-6)
    rig.animation_data.action = bpy.data.actions['walk']
    scene.frame_set(8)
    bpy.context.view_layer.update()
    matrices = np.array([np.array(top.matrix_world.inverted() @ rig.matrix_world @ rig.pose.bones[name].matrix @ rig.data.bones[name].matrix_local.inverted() @ rig.matrix_world.inverted() @ top.matrix_world) for name in names])
    blended = np.einsum('nk,nkij->nij', values, matrices[order])
    predicted = np.einsum('nij,nj->ni', blended[:, :3, :3], points) + blended[:, :3, 3]
    depsgraph = bpy.context.evaluated_depsgraph_get()
    evaluated = top.evaluated_get(depsgraph)
    evaluated_mesh = evaluated.to_mesh()
    actual = np.array([vertex.co[:] for vertex in evaluated_mesh.vertices])
    evaluated.to_mesh_clear()
    print('Skinning prediction max error:', np.linalg.norm(predicted - actual, axis=1).max(), flush=True)
    assert np.linalg.norm(predicted - actual, axis=1).max() < 1e-5
    arm_columns = [i for i, name in enumerate(names) if 'Arm' in name or 'Hand' in name]
    arm_skin = body_weights[:, arm_columns].sum(axis=1) > .35
    torso_faces = skin_faces[~arm_skin[skin_faces].any(axis=1)]
    poses = [(np.tile(np.eye(4), (len(names), 1, 1)), surface, None)]
    for clip in ['idle1', 'idle_armed', 'walk', 'run', 'jump', 'slash1', 'sit_idle', 'combat_idle', 'dying']:
        action = bpy.data.actions[clip]
        rig.animation_data.action = action
        for frame in np.linspace(*action.frame_range, 13):
            scene.frame_set(int(frame), subframe=float(frame % 1))
            bpy.context.view_layer.update()
            depsgraph = bpy.context.evaluated_depsgraph_get()
            evaluated_body = body.evaluated_get(depsgraph)
            evaluated_mesh = evaluated_body.to_mesh()
            posed_body = [evaluated_body.matrix_world @ vertex.co for vertex in evaluated_mesh.vertices]
            tree = BVHTree.FromPolygons(posed_body, torso_faces.tolist())
            full_tree = BVHTree.FromPolygons(posed_body, skin_faces.tolist())
            evaluated_body.to_mesh_clear()
            pose_matrices = np.array([np.array(rig.matrix_world @ rig.pose.bones[name].matrix @ rig.data.bones[name].matrix_local.inverted() @ rig.matrix_world.inverted()) for name in names])
            poses.append((pose_matrices, tree, full_tree))
    for iteration in range(12):
        correction = np.zeros((len(first), 3))
        counts = np.zeros(len(first))
        deepest = 0.0
        for matrices, tree, full_tree in poses:
            blended = np.einsum('nk,nkij->nij', values, matrices[order])
            posed = np.einsum('nij,nj->ni', blended[:, :3, :3], points) + blended[:, :3, 3]
            inverse_linear = np.linalg.inv(blended[:, :3, :3])
            for i, point in enumerate(posed):
                hit, normal, _, _ = tree.find_nearest(Vector(point))
                signed = np.dot(point - np.array(hit), np.array(normal))
                if signed < .003:
                    deepest = max(deepest, .003 - signed)
                    correction[welded[i]] += inverse_linear[i] @ (np.array(normal) * min(.003 - signed, .02))
                    counts[welded[i]] += 1
                if full_tree:
                    full_hit, full_normal, _, _ = full_tree.find_nearest(Vector(point))
                    full_signed = np.dot(point - np.array(full_hit), np.array(full_normal))
                    if full_signed < -.006:
                        correction[welded[i]] += .3 * (inverse_linear[i] @ (np.array(full_normal) * min(-.006 - full_signed, .02)))
                        counts[welded[i]] += 1
        correction /= np.maximum(counts[:, None], 1)
        for _ in range(2):
            updated = correction.copy()
            for i, neighbors in enumerate(adjacency):
                if neighbors:
                    updated[i] = .7 * correction[i] + .3 * correction[list(neighbors)].mean(0)
            correction = updated
        for label in np.unique(part_labels):
            selected = part_labels == label
            vertices = np.unique(welded[selected])
            if len(vertices) < 120:
                correction[vertices] = correction[vertices].mean(axis=0)
        lengths = np.linalg.norm(correction, axis=1)
        correction *= np.minimum(1, .004 / np.maximum(lengths, 1e-10))[:, None]
        points += correction[welded]
        print('Animation clearance', iteration + 1, 'maximum depth', deepest, flush=True)
    for vertex, point in zip(mesh.vertices, points):
        vertex.co = point
    mesh.update()
    rig.animation_data_clear()
    for bone in rig.pose.bones:
        bone.matrix_basis.identity()
    bpy.context.view_layer.update()
shell_tree = KDTree(len(shell_indices))
for i in shell_indices:
    shell_tree.insert(Vector(points[i]), int(i))
shell_tree.balance()
clearance_offset = np.zeros_like(points)
for i in shell_indices:
    front = np.clip((-points[i, 1] - .015) / .07, 0, 1)
    front = front * front * (3 - 2 * front)
    _, normal, _, _ = surface.find_nearest(Vector(points[i]))
    clearance_offset[i] = np.array(normal) * (.009 * front)
for label in np.unique(source_labels):
    if label == shell_label:
        continue
    selected = np.flatnonzero(source_labels == label)
    anchor = min((shell_tree.find(Vector(points[i])) for i in selected), key=lambda value: value[2])[1]
    clearance_offset[selected] = clearance_offset[anchor]
points += clearance_offset
shell_surface = BVHTree.FromPolygons([Vector(point) for point in points], shell_faces.tolist())
for fang in fangs:
    selected = source_labels == fang['label']
    current = points[selected]
    high = np.quantile(current[:, 2], .9)
    root = current[current[:, 2] >= high].mean(axis=0)
    hit, normal, _, distance = shell_surface.find_nearest(Vector(root))
    hit, normal = np.array(hit), np.array(normal)
    if np.dot(root - hit, normal) < 0:
        normal = -normal
    displacement = hit + normal * .004 - root
    if fang['type'] == 'upper':
        lower, upper = current[:, 2].min(), current[:, 2].max()
        blend = np.clip((current[:, 2] - (lower + .65 * (upper - lower))) / max(.35 * (upper - lower), 1e-8), 0, 1)
        blend = blend * blend * (3 - 2 * blend)
    else:
        blend = np.ones(len(current))
    offsets = blend[:, None] * displacement

    def skin_clearance(candidate):
        depths = []
        for point in candidate:
            location, direction, _, _ = surface.find_nearest(Vector(point))
            depths.append(np.dot(point - np.array(location), np.array(direction)))
        return min(depths)

    clearance = min(.001, skin_clearance(current))
    low, high = 0.0, 1.0
    for _ in range(12):
        mid = (low + high) / 2
        if skin_clearance(current + offsets * mid) >= clearance - .0001:
            low = mid
        else:
            high = mid
    points[selected] += offsets * low
    fang['root_distance_before_m'] = float(distance)
    moved_root = points[selected][current[:, 2] >= np.quantile(current[:, 2], .9)].mean(axis=0)
    fang['root_distance_after_m'] = float(shell_surface.find_nearest(Vector(moved_root))[3])
    fang['snap_fraction'] = float(low)
    if fang['type'] == 'rear':
        current = points[selected]
        threshold = np.quantile(current[:, 2], .1)
        tip = current[current[:, 2] <= threshold].mean(axis=0)
        hit, normal, _, distance = shell_surface.find_nearest(Vector(tip))
        hit, normal = np.array(hit), np.array(normal)
        if np.dot(tip - hit, normal) < 0:
            normal = -normal
        displacement = hit + normal * .004 - tip
        lower, upper = current[:, 2].min(), current[:, 2].max()
        blend = np.clip((upper - current[:, 2]) / max(upper - lower, 1e-8), 0, 1)
        blend = blend * blend * (3 - 2 * blend)
        offsets = blend[:, None] * displacement
        clearance = min(.001, skin_clearance(current))
        low, high = 0.0, 1.0
        for _ in range(12):
            mid = (low + high) / 2
            if skin_clearance(current + offsets * mid) >= clearance - .0001:
                low = mid
            else:
                high = mid
        points[selected] += offsets * low
        moved_tip = points[selected][current[:, 2] <= threshold].mean(axis=0)
        fang['tip_distance_before_m'] = float(distance)
        fang['tip_distance_after_m'] = float(shell_surface.find_nearest(Vector(moved_tip))[3])
        fang['tip_snap_fraction'] = float(low)
for fang in fangs:
    if fang['type'] != 'pendant':
        continue
    selected = source_labels == fang['label']
    current = points[selected]
    low, high = np.quantile(current[:, 2], [.1, .9])
    tip = current[current[:, 2] <= low].mean(axis=0)
    root = current[current[:, 2] >= high].mean(axis=0)
    sign = -1 if np.median(source[selected, 0]) < 0 else 1
    root_x, root_z = sign * .008, 1.38
    origin = Vector((root_x, .05, root_z))
    direction = Vector((0, -1, 0))
    cloth_hit = shell_surface.ray_cast(origin, direction, .35)[0]
    skin_hit = surface.ray_cast(origin, direction, .35)[0]
    assert cloth_hit is not None and skin_hit is not None and cloth_hit.y < skin_hit.y
    root_target = np.array([root_x, (cloth_hit.y + skin_hit.y) / 2, root_z])
    tip_reference = tip + np.array([0, 0, .025])
    hit, normal, _, _ = surface.find_nearest(Vector(tip_reference))
    tip_target = np.array(hit) + np.array(normal) * .007
    old_axis, new_axis = root - tip, root_target - tip_target
    axis = old_axis / np.linalg.norm(old_axis)
    rotation = np.array(Vector(old_axis).rotation_difference(Vector(new_axis)).to_matrix())
    local = current - tip
    along = local @ axis
    perpendicular = local - along[:, None] * axis
    ratio = np.clip(np.linalg.norm(new_axis) / np.linalg.norm(old_axis), .9, 1.1)
    updated = tip_target + (rotation @ (perpendicular + along[:, None] * axis * ratio).T).T

    stations = np.linspace(0, 1, 9)
    centers = []
    for station in stations:
        reference = tip_target * (1 - station) + root_target * station
        origin = Vector((reference[0], .05, reference[2]))
        hit = surface.ray_cast(origin, direction, .35)[0]
        skin_y = hit.y - .007 if hit is not None else reference[1]
        blend = np.clip((station - .72) / .18, 0, 1)
        blend = blend * blend * (3 - 2 * blend)
        centers.append(skin_y * (1 - blend) + reference[1] * blend)
    progress = np.clip((updated[:, 2] - tip_target[2]) / (root_target[2] - tip_target[2]), 0, 1)
    old_center_y = tip_target[1] * (1 - progress) + root_target[1] * progress
    updated[:, 1] += np.interp(progress, stations, centers) - old_center_y
    points[selected] = updated
    hit, _, face_index, _ = shell_surface.find_nearest(Vector(root_target))
    vertices = shell_faces[face_index]
    a, b, c = points[vertices]
    uv = np.linalg.lstsq(np.column_stack((b - a, c - a)), np.array(hit) - a, rcond=None)[0]
    bary = np.clip([1 - uv.sum(), *uv], 0, 1)
    bary /= bary.sum()
    attachment_weights = bary @ weights[vertices]
    fitted_tip = updated[updated[:, 2] <= np.quantile(updated[:, 2], .1)].mean(axis=0)
    hit, _, face_index, _ = surface.find_nearest(Vector(fitted_tip))
    vertices = skin_faces[face_index]
    a, b, c = bp[vertices]
    uv = np.linalg.lstsq(np.column_stack((b - a, c - a)), np.array(hit) - a, rcond=None)[0]
    bary = np.clip([1 - uv.sum(), *uv], 0, 1)
    bary /= bary.sum()
    tip_weights = bary @ body_weights[vertices]
    selected_vertices = np.flatnonzero(selected)
    blend = np.clip((root_target[2] - updated[:, 2]) / max(root_target[2] - fitted_tip[2], 1e-8), 0, 1)
    blend = blend * blend * (3 - 2 * blend)
    weights[selected] = (1 - blend[:, None]) * attachment_weights + blend[:, None] * tip_weights
    order[selected] = np.argsort(weights[selected], axis=1)[:, -4:][:, ::-1]
    values[selected] = np.take_along_axis(weights[selected], order[selected], axis=1)
    values[selected] = np.where(values[selected] < .005, 0, values[selected])
    values[selected] /= values[selected].sum(axis=1, keepdims=True)
    order[selected] = np.where(values[selected] == 0, 0, order[selected])
    for group in top.vertex_groups:
        group.remove(selected_vertices.tolist())
    for vertex in selected_vertices:
        for joint, weight in zip(order[vertex], values[vertex]):
            if weight:
                top.vertex_groups[int(joint)].add([int(vertex)], float(weight), 'REPLACE')
    fang['tucked_root_blender'] = root_target.tolist()
    fang['cloth_y_at_root'] = float(cloth_hit.y)
    fang['skin_y_at_root'] = float(skin_hit.y)
    fang['tuck_length_ratio'] = float(ratio)
for vertex, point in zip(mesh.vertices, points):
    vertex.co = point
mesh.update()
face_normals = np.cross(points[faces[:, 1]] - points[faces[:, 0]], points[faces[:, 2]] - points[faces[:, 0]])
normal_sum = np.zeros((len(first), 3))
for k in range(3):
    np.add.at(normal_sum, welded[faces[:, k]], face_normals)
normal_sum /= np.maximum(np.linalg.norm(normal_sum, axis=1, keepdims=True), 1e-12)
mesh.normals_split_custom_set([normal_sum[welded[loop.vertex_index]] for loop in mesh.loops])
for key, data in [('_RIG_JOINTS', order), ('_RIG_WEIGHTS', values)]:
    attribute = mesh.attributes.new(key, 'FLOAT_COLOR', 'POINT')
    attribute.data.foreach_set('color', data.astype(np.float32).ravel())
bpy.ops.object.select_all(action='DESELECT')
top.select_set(True)
bpy.context.view_layer.objects.active = top
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
canonical_names = [base['nodes'][j]['name'] for j in base['skins'][0]['joints']]
remap_order = np.array([canonical_names.index(name) for name in names])
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
doc['nodes'].append({'name': 'top_female_caveman', 'mesh': 0, 'skin': 0, 'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'top_female_caveman'}})
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
write_glb(OUTPUT / 'top_female_caveman.glb', doc, raw)
intermediate.unlink()
np.savez(OUTPUT / 'fit-data.npz', source=source, points=points, faces=faces, joints=order, weights=values, bone_names=np.array(names))

underwear = np.zeros(len(bf), dtype=bool)
for label in np.unique(labels):
    q = bp[labels == label]
    if label != skin_label and q[:, 2].min() > 1.15 and q[:, 2].max() < 1.55:
        underwear |= (labels[bf] == label).all(1)
edit = bmesh.new()
visual_body = body.copy()
visual_body.data = body.data.copy()
bpy.context.collection.objects.link(visual_body)
edit.from_mesh(visual_body.data)
edit.faces.ensure_lookup_table()
bmesh.ops.delete(edit, geom=[edit.faces[i] for i in np.flatnonzero(underwear)], context='FACES')
edit.to_mesh(visual_body.data)
edit.free()
body.hide_render = True
body.hide_set(True)
bpy.ops.file.pack_all()
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'female-caveman-top.blend'))
scene.render.engine = 'BLENDER_WORKBENCH'
scene.display.shading.color_type = 'TEXTURE'
scene.display.shading.light = 'STUDIO'
scene.render.resolution_x = scene.render.resolution_y = 900
bpy.ops.object.camera_add()
camera = bpy.context.object
camera.data.type = 'ORTHO'
camera.data.ortho_scale = .64
scene.camera = camera
(OUTPUT / 'review').mkdir(exist_ok=True)
for name, offset in [('front', (0, -3, 0)), ('back', (0, 3, 0)), ('side', (3, 0, 0)), ('angle', (2, -3, .2))]:
    center = Vector((0, 0, 1.37))
    camera.location = center + Vector(offset)
    camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.filepath = str(OUTPUT / 'review' / f'{name}.png')
    bpy.ops.render.render(write_still=True)
shell_clearance = []
for point in points[shell_indices]:
    hit, normal, _, _ = surface.find_nearest(Vector(point))
    shell_clearance.append(float(np.dot(point - np.array(hit), np.array(normal))))
report = {'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
          'triangles': len(faces), 'vertices': len(points), 'bones': len(names), 'date': '2026-11-01',
          'method': 'Skin clearance projection, shell refitting and accessory alignment, barycentric skin weight transfer, canonical hierarchy and exact inverse bind matrices.',
          'fangs': fangs,
          'rest_shell_clearance_m': {'min': float(np.min(shell_clearance)), 'median': float(np.median(shell_clearance)),
                                     'p90': float(np.percentile(shell_clearance, 90)), 'max': float(np.max(shell_clearance))},
          'animation_clearance': '--animation-clearance' in sys.argv,
          'no_new_api_requests': True, 'status': 'Candidate; visual and animation review pending'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
