"""Rig and animate Medusa from her Tripo source.

    blender -b -P tools/blender-scripts/rig_medusa.py -- [--review DIR]

"""
import argparse
import json
import math
import sys
from dataclasses import dataclass
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/monsters/medusa/tripo_v1/source.glb'
RIG_DIR = ROOT / 'assets/monsters/medusa/rig_v1'
GAME = ROOT / 'client/public/models/monsters/medusa.glb'
HEIGHT = 2.4
FPS = 30
TEXTURE = 1024

# Source landmarks, in the unit-height source with Blender axes (front -Y).
CX = -0.149
TORSO_Y = -0.33
ARM_JOINTS = [0.105, 0.215, 0.325]
ARM_END = 0.384
TRUNK_CUTS = [0.16, 0.28, 0.40, 0.50, 0.60, 0.70, 0.835, 0.87]
TRUNK_BLEND = [0.05, 0.05, 0.05, 0.04, 0.04, 0.04, 0.02, 0.015]
TRUNK = ['Col1', 'Col2', 'Col3', 'Col4', 'Hips', 'Spine', 'Chest', 'Neck', 'Head']
GROUND_Z = 0.115
TAIL_BONES = 8


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


def chain_weights(param, cuts, blends, count):
    """Weights of `count` bones split along `param` at `cuts`."""
    steps = [smoothstep(c - b, c + b, param) for c, b in zip(cuts, blends)]
    weights = np.zeros((len(param), count))
    for i in range(count):
        past = steps[i - 1] if i else 1
        ahead = steps[i] if i < count - 1 else 0
        weights[:, i] = past * (1 - ahead)
    return weights / weights.sum(axis=1, keepdims=True)


def load_mesh():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(SOURCE))
    meshes = [o for o in bpy.context.scene.objects if o.type == 'MESH']
    assert len(meshes) == 1
    mesh = meshes[0]
    for other in [o for o in bpy.context.scene.objects if o is not mesh]:
        bpy.data.objects.remove(other)
    bpy.context.view_layer.objects.active = mesh
    mesh.select_set(True)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    mesh.name = mesh.data.name = 'Medusa'
    # Two hair-thin flat slivers reach out of the brow.
    bm = bmesh.new()
    bm.from_mesh(mesh.data)
    bm.verts.ensure_lookup_table()
    seen, stray = set(), []
    for start in bm.verts:
        if start.index in seen:
            continue
        island, stack = [], [start]
        seen.add(start.index)
        while stack:
            vert = stack.pop()
            island.append(vert)
            for edge in vert.link_edges:
                other = edge.other_vert(vert)
                if other.index not in seen:
                    seen.add(other.index)
                    stack.append(other)
        heights = [v.co.z for v in island]
        if len(island) < 400 and 0.925 < min(heights) and max(heights) < 0.93 and min(v.co.y for v in island) < -0.335:
            stray.extend(island)
    bmesh.ops.delete(bm, geom=stray, context='VERTS')
    for vert in bm.verts:
        vert.co = Vector(((vert.co.x - CX) * HEIGHT, (vert.co.y - TORSO_Y) * HEIGHT, vert.co.z * HEIGHT))
    bm.to_mesh(mesh.data)
    bm.free()
    for image in bpy.data.images:
        if image.size[0] > TEXTURE:
            image.scale(TEXTURE, TEXTURE)
    return mesh, len(stray)


def unit_points(mesh):
    points = np.zeros(len(mesh.data.vertices) * 3)
    mesh.data.vertices.foreach_get('co', points)
    return points.reshape(-1, 3) / HEIGHT


def measure(points):
    """Joint positions in unit space, origin under the torso."""
    x, y, z = points.T
    near_axis = np.abs(x) < 0.09

    def trunk(height):
        ring = near_axis & (np.abs(z - height) < 0.03)
        if ring.sum() < 8:
            ring = near_axis & (np.abs(z - height) < 0.07)
        return np.array([0, np.median(y[ring]), height])

    arm = (np.abs(x) > 0.15) & (z > 0.74) & (z < 0.87)
    arm_y, arm_z = np.median(y[arm]), np.median(z[arm])
    base = trunk(0.13)
    low = (z < GROUND_Z) | (y > 0.22)
    ground = points[low]
    tip = ground[np.linalg.norm(ground[:, :2] - base[:2], axis=1).argmax()]
    axis = tip[:2] - base[:2]
    depth = (ground[:, :2] - base[:2]) @ axis / (axis @ axis)
    nodes = [np.array([base[0], base[1], 0.05])]
    for section in np.arange(1, TAIL_BONES) / TAIL_BONES:
        ring = ground[np.abs(depth - section) < 0.045]
        nodes.append((ring.max(axis=0) + ring.min(axis=0)) * 0.5)
    nodes.append(tip)
    edges = np.diff(np.array(nodes)[:, :2], axis=0)
    lengths = np.linalg.norm(edges, axis=1)
    stations = np.r_[0, np.cumsum(lengths)]
    offsets = points[:, None, :2] - np.array(nodes)[None, :-1, :2]
    along = np.clip((offsets * edges).sum(axis=2) / lengths ** 2, 0, 1)
    nearest = np.linalg.norm(offsets - along[:, :, None] * edges, axis=2).argmin(axis=1)
    distance = stations[nearest] + along[np.arange(len(points)), nearest] * lengths[nearest]
    heights = [0.05] + TRUNK_CUTS + [1.0]
    joints = [trunk(h) for h in heights[:-1]] + [np.array([0, trunk(0.9)[1], 1.0])]
    joints[0] = nodes[0]
    return {'trunk': joints, 'arm': (arm_y, arm_z), 'tail': nodes,
            'tail_distance': distance, 'tail_stations': stations}


def build_armature(layout):
    data = bpy.data.armatures.new('MedusaRig')
    rig = bpy.data.objects.new('MedusaRig', data)
    bpy.context.scene.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')

    def bone(name, head, tail, parent=None):
        b = data.edit_bones.new(name)
        b.head, b.tail = Vector(head) * HEIGHT, Vector(tail) * HEIGHT
        b.parent = data.edit_bones[parent] if parent else None
        return b

    bone('Root', (0, 0, 0), (0, 0, 0.08))
    trunk = layout['trunk']
    for i, name in enumerate(TRUNK):
        bone(name, trunk[i], trunk[i + 1], TRUNK[i - 1] if i else 'Root')
    arm_y, arm_z = layout['arm']
    for side, sign in [('Left', 1), ('Right', -1)]:
        bone(side + 'Snakes', (sign * 0.045, 0.06, 0.905),
             (sign * 0.117, 0.05, 0.935), 'Head')
        cuts = [0.03] + ARM_JOINTS + [ARM_END]
        parent = 'Chest'
        for name, near, far in zip(['Shoulder', 'Arm', 'ForeArm', 'Hand'], cuts[:-1], cuts[1:]):
            bone(side + name, (sign * near, arm_y, arm_z), (sign * far, arm_y, arm_z), parent)
            parent = side + name
    bone('CrownSnake', (0, 0.065, 0.945), (0, 0.04, 0.997), 'Head')
    tail = layout['tail']
    for i in range(TAIL_BONES):
        bone(f'Tail{i + 1}', tail[i], tail[i + 1], f'Tail{i}' if i else 'Root')
    bpy.ops.object.mode_set(mode='OBJECT')
    return rig


def skin(mesh, rig, points, layout):
    x, y, z = points.T
    names = [b.name for b in rig.data.bones]
    dense = np.zeros((len(points), len(names)))
    column = {name: names.index(name) for name in names}

    trunk = chain_weights(z, TRUNK_CUTS, TRUNK_BLEND, len(TRUNK))
    side = np.abs(x)
    arm = chain_weights(side, ARM_JOINTS, [0.02, 0.03, 0.02], 4)
    in_arm = smoothstep(0.07, 0.11, side) * ((z > 0.745) & (z < 0.829))
    for i, name in enumerate(TRUNK):
        dense[:, column[name]] += trunk[:, i] * (1 - in_arm)
    for prefix, mask in [('Left', x > 0), ('Right', x <= 0)]:
        for i, name in enumerate(['Shoulder', 'Arm', 'ForeArm', 'Hand']):
            dense[:, column[prefix + name]] += arm[:, i] * in_arm * mask
    distance = layout['tail_distance']
    stations = layout['tail_stations']
    lengths = np.diff(stations)
    widths = 0.48 * np.minimum(lengths[:-1], lengths[1:])
    ground = chain_weights(distance, stations[1:-1], widths, TAIL_BONES)
    tail_mask = np.maximum(1 - smoothstep(GROUND_Z, 0.15, z), smoothstep(0.18, 0.22, y))
    on_ground = smoothstep(0.01, 0.13, distance) * tail_mask
    dense *= 1 - on_ground[:, None]
    for i in range(TAIL_BONES):
        dense[:, column[f'Tail{i + 1}']] += ground[:, i] * on_ground

    side_snakes = smoothstep(0.04, 0.078, side) * smoothstep(0.83, 0.853, z)
    crown_snake = smoothstep(0.946, 0.979, z) * (1 - smoothstep(0.016, 0.044, side))
    dense *= (1 - side_snakes - crown_snake)[:, None]
    dense[:, column['LeftSnakes']] = side_snakes * (x > 0)
    dense[:, column['RightSnakes']] = side_snakes * (x < 0)
    dense[:, column['CrownSnake']] = crown_snake

    edge_ids = np.zeros(len(mesh.data.edges) * 2, dtype=int)
    mesh.data.edges.foreach_get('vertices', edge_ids)
    edge_ids = edge_ids.reshape(-1, 2)
    # Seam vertices share a place, not an edge; average them together.
    keys = np.round(points * 1e5).astype(np.int64)
    _, groups = np.unique(keys, axis=0, return_inverse=True)
    groups = groups.reshape(-1)
    sizes = np.bincount(groups)
    for _ in range(4):
        total = np.zeros_like(dense)
        count = np.zeros(len(points))
        for a, b in [(0, 1), (1, 0)]:
            np.add.at(total, edge_ids[:, a], dense[edge_ids[:, b]])
            np.add.at(count, edge_ids[:, a], 1)
        averaged = np.where(count[:, None] > 0, total / np.maximum(count, 1)[:, None], dense)
        dense = 0.5 * dense + 0.5 * averaged
        merged = np.zeros((len(sizes), len(names)))
        np.add.at(merged, groups, dense)
        dense = merged[groups] / sizes[groups, None]
    order = np.argsort(-dense, axis=1)[:, :4]
    kept = np.take_along_axis(dense, order, axis=1)
    kept /= kept.sum(axis=1, keepdims=True)
    groups_by_name = [mesh.vertex_groups.new(name=name) for name in names]
    for vertex in range(len(points)):
        for bone_index, weight in zip(order[vertex], kept[vertex]):
            if weight > 1e-4:
                groups_by_name[bone_index].add([vertex], float(weight), 'REPLACE')
    modifier = mesh.modifiers.new('Armature', 'ARMATURE')
    modifier.object = rig
    mesh.parent = rig


def rot(axis, degrees):
    return Matrix.Rotation(math.radians(degrees), 3, axis)


@dataclass
class Motion:
    bones: dict
    headings: list
    shift_x: float = 0.0
    elevation: tuple = ()


class Poser:
    """Forward kinematics over rotations given in the rest pose's world axes."""

    def __init__(self, rig):
        self.rig = rig
        self.order = [b.name for b in rig.data.bones]
        self.rest = {b.name: b.matrix_local.copy() for b in rig.data.bones}
        self.parent = {b.name: b.parent.name if b.parent else None for b in rig.data.bones}
        self.previous = {}
        tail = [rig.data.bones[f'Tail{i + 1}'] for i in range(TAIL_BONES)]
        self.tail_yaw = [math.atan2(b.tail_local.y - b.head_local.y, b.tail_local.x - b.head_local.x) for b in tail]

    def tail_pose(self, pose, yaws, elevation):
        last = Matrix.Identity(3)
        for i, (target, rest) in enumerate(zip(yaws, self.tail_yaw)):
            turn = Matrix.Rotation(target - rest, 3, 'Z')
            if elevation:
                axis = Vector((math.sin(target), -math.cos(target), 0))
                turn = Matrix.Rotation(math.radians(elevation[i]), 3, axis) @ turn
            pose[f'Tail{i + 1}'] = last.transposed() @ turn
            last = turn
        twist = yaws[0] - self.tail_yaw[0]
        # The trunk shares the turn, so the bend at the ground never wrings.
        for name, share in [('Col1', 2 / 3), ('Col2', -1 / 3), ('Col3', -1 / 3)]:
            pose[name] = Matrix.Rotation(twist * share, 3, 'Z') @ pose.get(name, Matrix.Identity(3))

    def key(self, frame, pose, shift_x):
        world_rot, world_head, matrices = {}, {}, {}
        for name in self.order:
            parent = self.parent[name]
            delta = pose.get(name, Matrix.Identity(3))
            if parent:
                world_rot[name] = world_rot[parent] @ delta
                offset = self.rest[name].translation - self.rest[parent].translation
                world_head[name] = world_head[parent] + world_rot[parent] @ offset
            else:
                world_rot[name] = delta
                world_head[name] = self.rest[name].translation + Vector((shift_x, 0, 0))
            matrices[name] = Matrix.Translation(world_head[name]) @ (world_rot[name] @ self.rest[name].to_3x3()).to_4x4()
        for name in self.order:
            bone = self.rig.pose.bones[name]
            parent = self.parent[name]
            if parent:
                basis = (self.rest[parent].inverted() @ self.rest[name]).inverted() @ (matrices[parent].inverted() @ matrices[name])
            else:
                basis = self.rest[name].inverted() @ matrices[name]
            bone.matrix_basis = basis
            quaternion = bone.rotation_quaternion
            if name in self.previous and quaternion.dot(self.previous[name]) < 0:
                quaternion.negate()
            self.previous[name] = quaternion.copy()
            bone.keyframe_insert('rotation_quaternion', frame=frame, group=name)
            if not parent:
                bone.keyframe_insert('location', frame=frame, group=name)


def ease(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def span(t, start, end):
    return ease((t - start) / (end - start))


def arms(pose, drop, bend):
    for side, sign in [('Left', 1), ('Right', -1)]:
        pose[side + 'Arm'] = rot('Y', sign * drop)
        pose[side + 'ForeArm'] = rot('Z', -sign * bend)
        pose[side + 'Hand'] = rot('Z', -sign * bend * 0.25)


def trail(wave, amplitude, speed, heading=90.0, curl=0.0):
    """Headings of a tail trailing behind, a wave running down it."""
    def at(t):
        return [math.radians(heading + curl * i) + amplitude * (0.35 + 0.65 * i / (TAIL_BONES - 1))
                * math.sin(wave * i - speed * t * math.tau) for i in range(TAIL_BONES)]
    return at


def lean_back(pose, degrees):
    for joint, share in [('Hips', -0.6), ('Spine', -0.4), ('Neck', 1.0)]:
        pose[joint] = rot('X', degrees * share) @ pose.get(joint, Matrix.Identity(3))


def clip_idle(t):
    pose = {}
    sway = math.sin(t / 4 * math.tau)
    pose['Col2'] = rot('Y', 1.5 * sway)
    pose['Col4'] = rot('Y', -1.0 * sway)
    pose['Chest'] = rot('X', 1.5 * math.sin(t / 2 * math.tau))
    pose['Head'] = rot('Z', 6 * math.sin(t / 4 * math.tau + 1))
    arms(pose, 62, 18)
    lean_back(pose, 8)
    return Motion(pose, trail(0.9, 0.22, 0.25)(t))


def clip_combat_idle(t, attack_weight=0.0):
    pose = {}
    pulse = math.sin(t / 2 * math.tau)
    pose['Col3'] = rot('X', 5)
    pose['Spine'] = rot('X', 6 + 1.5 * pulse)
    pose['Head'] = rot('X', -8)
    arms(pose, 38 + 2 * pulse, 62)
    lean_back(pose, 12 * (1 - attack_weight))
    return Motion(pose, trail(1.0, 0.3, 0.5)(t))


def aim_arm(pose, side, upper, fore, weight):
    sign = 1 if side == 'Left' else -1

    def direction(forward, down):
        return rot('Z', -sign * forward) @ rot('Y', sign * down)

    shoulder = direction(*upper)
    elbow = shoulder.transposed() @ direction(*fore)
    for joint, target in [('Arm', shoulder), ('ForeArm', elbow), ('Hand', rot('Y', -sign * 12))]:
        key = side + joint
        pose[key] = pose[key].lerp(target, weight)


def moving(period, wavelength, width, lean, stations):
    def clip(t):
        phase = math.tau * t / period
        wave = math.sin(phase)
        lateral = [width * (0.35 + 0.65 * ease(s / 0.95)) *
                   math.sin(phase - math.tau * s / wavelength) for s in stations]
        headings = [math.acos((b - a) / (end - start))
                    for a, b, start, end in zip(lateral, lateral[1:], stations, stations[1:])]
        pose = {
            'Col2': rot('X', lean) @ rot('Y', -7 * wave),
            'Col4': rot('Y', 9 * math.sin(phase - 0.45)),
            'Hips': rot('Z', 9 * math.cos(phase)),
            'Chest': rot('Z', -12 * math.cos(phase - 0.25)) @ rot('X', 3),
            'Head': rot('X', -lean - 3) @ rot('Y', -3 * wave),
        }
        arms(pose, 48, 52)
        for side, sign in [('Left', 1), ('Right', -1)]:
            beat = sign * math.sin(phase - 0.4)
            aim_arm(pose, side, (20 + 19 * beat, 56 - 7 * beat),
                    (64 + 20 * beat, 21 - 9 * beat), 1)
            pose[side + 'Snakes'] = rot('Y', -sign * 5 * math.sin(phase - 0.7))
        return Motion(pose, headings, lateral[0])
    return clip


def clip_claw(t):
    recoil = span(t, 0, 0.20) * (1 - span(t, 0.20, 0.36))
    strike = span(t, 0.20, 0.36) * (1 - span(t, 0.39, 0.88))
    motion = clip_combat_idle(0, recoil + strike)
    pose = motion.bones
    pose['Col2'] = rot('X', -9 * recoil + 16 * strike)
    pose['Col4'] = rot('X', -4 * recoil + 14 * strike)
    pose['Chest'] = rot('Z', 18 * recoil - 11 * strike) @ rot('X', 9 * strike)
    pose['Neck'] = pose['Neck'] @ rot('X', 8 * recoil - 12 * strike)
    pose['Head'] = rot('X', -8 - 7 * recoil - 8 * strike) @ rot('Z', -12 * recoil + 8 * strike)
    aim_arm(pose, 'Right', (8, 30), (95, -30), recoil)
    aim_arm(pose, 'Right', (-18, 52), (18, 42), strike)
    aim_arm(pose, 'Left', (32, 38), (90, 5), strike)
    pose['RightSnakes'] = rot('Z', -20 * recoil + 67 * strike) @ rot('Y', 15 * recoil)
    pose['LeftSnakes'] = rot('Z', -32 * strike)
    pose['CrownSnake'] = rot('X', -18 * recoil + 38 * strike)
    return motion


def clip_gaze(t):
    gather = span(t, 0, 0.85) * (1 - span(t, 0.92, 2.15))
    fan = span(t, 0.70, 2.15) * (1 - span(t, 3.38, 4.0))
    stare = span(t, 2.68, 3.0) * (1 - span(t, 3.34, 3.94))
    motion = clip_combat_idle(0, min(1, gather + fan + stare))
    pose = motion.bones
    pose['Col2'] = rot('X', -11 * fan + 22 * stare)
    pose['Chest'] = rot('X', 12 * gather - 9 * fan + 15 * stare)
    pose['Head'] = rot('X', -8 + 19 * gather + 12 * fan - 23 * stare)
    for side, sign in [('Left', 1), ('Right', -1)]:
        aim_arm(pose, side, (54, 42), (126, -64), gather)
        aim_arm(pose, side, (0, 12), (26, -62), fan)
        aim_arm(pose, side, (-14, 21), (12, -27), stare)
        pose[side + 'Snakes'] = rot('Z', -sign * 48 * stare) @ rot('Y', -sign * (28 * fan + 6 * gather))
    pose['CrownSnake'] = rot('X', -26 * fan + 56 * stare)
    motion.headings = [h + math.radians(5 * i * fan) for i, h in enumerate(motion.headings)]
    return motion


def clip_tail_sweep(t):
    drive = span(t, 1.41, 1.62)
    gather = span(t, 0.06, 1.19) * (1 - drive)
    follow = drive * (1 - span(t, 1.79, 2.52))
    motion = clip_combat_idle(0, gather + follow)
    pose = motion.bones
    pose['Col3'] = rot('X', 5 + 12 * gather) @ rot('Y', -11 * gather + 13 * follow)
    pose['Hips'] = pose['Hips'] @ rot('Z', -25 * gather + 22 * follow)
    pose['Chest'] = rot('Z', 48 * gather - 52 * follow)
    pose['Head'] = rot('X', -8) @ rot('Z', 43 * gather - 12 * follow)
    aim_arm(pose, 'Left', (-13, 28), (12, -8), gather)
    aim_arm(pose, 'Right', (38, 28), (108, -16), gather)
    aim_arm(pose, 'Right', (-19, 24), (14, 6), follow)
    curled = [40, 16, -8, -20, -3, 26, 63, 103]
    crossed = [142, 154, 166, 174, 180, 186, 194, 205]
    elevations = []
    for i, neutral in enumerate(motion.headings):
        lag = 0.010 * i
        wind = span(t - lag, 0.04, 1.18)
        lash = span(t - lag, 1.475, 1.685)
        rebound = span(t - lag, 1.71, 1.84)
        relax = span(t - lag, 1.92, 2.48)
        start = neutral + (math.radians(curled[i]) - neutral) * wind
        swept = start + (math.radians(crossed[i]) - start) * lash
        swept -= math.radians(2 * i) * rebound
        motion.headings[i] = swept + (neutral - swept) * relax
        elevations.append(((3 + i * 1.3) * wind - (1 + i * 0.8) * lash) * (1 - relax))
    motion.elevation = tuple(elevations)
    return motion


def clip_dying(t):
    """Topples sideways and lies still."""
    pose = {}
    fall = span(t, 0.1, 1.2)
    fall = fall * fall
    for name, share in [('Col1', 26), ('Col2', 26), ('Col3', 22), ('Col4', 14)]:
        pose[name] = rot('Y', share * fall)
    pose['Spine'] = rot('Y', 6 * fall) @ rot('X', -10 * fall)
    pose['Head'] = rot('Y', 12 * fall) @ rot('X', 18 * span(t, 0.0, 0.3) * (1 - fall))
    slack = span(t, 0.0, 0.9)
    arms(pose, 38 + 30 * slack, 62 - 40 * slack)
    calm = 1 - span(t, 0.2, 1.4)
    return Motion(pose, trail(1.0, 0.3 * calm + 0.05, 0.5 * calm)(t))


def clips_for(rig):
    stations = [0.0]
    for i in range(TAIL_BONES):
        bone = rig.data.bones[f'Tail{i + 1}']
        stations.append(stations[-1] + (bone.tail_local - bone.head_local).xy.length)
    return [
        ('idle1', 4.0, clip_idle),
        ('combat_idle', 2.0, clip_combat_idle),
        ('walk', 1.4, moving(1.4, 2.35, 0.25, 5, stations)),
        ('run', 0.8, moving(0.8, 3.65, 0.40, 12, stations)),
        ('claw', 1.0, clip_claw),
        ('gaze', 4.0, clip_gaze),
        ('tail_sweep', 2.6, clip_tail_sweep),
        ('dying', 1.6, clip_dying),
    ]


def bake(rig, clips):
    bpy.context.scene.render.fps = FPS
    rig.animation_data_create()
    poser = Poser(rig)
    for bone in rig.pose.bones:
        bone.rotation_mode = 'QUATERNION'
    actions = {}
    for name, duration, clip in clips:
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        rig.animation_data.action = action
        poser.previous = {}
        frames = round(duration * FPS)
        for frame in range(frames + 1):
            motion = clip(frame / FPS)
            poser.tail_pose(motion.bones, motion.headings, motion.elevation)
            poser.key(frame, motion.bones, motion.shift_x)
        if action.slots:
            rig.animation_data.action_slot = action.slots[0]
        actions[name] = action
    return actions


def review(rig, actions, directory):
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_EEVEE'
    scene.render.resolution_x = scene.render.resolution_y = 640
    scene.world = bpy.data.worlds.new('w')
    scene.world.use_nodes = True
    background = scene.world.node_tree.nodes['Background']
    background.inputs[0].default_value = (0.5, 0.5, 0.5, 1)
    background.inputs[1].default_value = 1.3
    sun = bpy.data.objects.new('sun', bpy.data.lights.new('sun', 'SUN'))
    sun.data.energy = 3
    sun.rotation_euler = (0.9, 0, 0.5)
    scene.collection.objects.link(sun)
    camera = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
    scene.collection.objects.link(camera)
    scene.camera = camera
    camera.data.type = 'ORTHO'
    camera.data.ortho_scale = 5.2
    # The game's view: from the south-west, looking down.
    target = Vector((0, 0.4, 1.0))
    camera.location = target + Vector((-1, -1, 1.15)).normalized() * 30
    camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
    directory.mkdir(parents=True, exist_ok=True)
    shots = {'idle1': [0, 2.0], 'combat_idle': [0.5], 'walk': [0.0, 0.8], 'run': [0.25, 0.75],
             'claw': [0.2, 0.36, 0.6], 'gaze': [0.7, 2.5, 3.0], 'tail_sweep': [1.2, 1.45, 1.6, 1.8, 2.3],
             'dying': [0.6, 1.6]}
    for name, times in shots.items():
        rig.animation_data.action = actions[name]
        if actions[name].slots:
            rig.animation_data.action_slot = actions[name].slots[0]
        for t in times:
            scene.frame_set(round(t * FPS))
            scene.render.filepath = str(directory / f'{name}_{t:.2f}.png')
            bpy.ops.render.render(write_still=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--review', type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    mesh, stray = load_mesh()
    points = unit_points(mesh)
    layout = measure(points)
    rig = build_armature(layout)
    skin(mesh, rig, points, layout)
    clips = clips_for(rig)
    actions = bake(rig, clips)
    bpy.context.scene.frame_start = 0
    bpy.context.scene.frame_end = 4 * FPS
    rig.animation_data.action = None
    for bone in rig.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    RIG_DIR.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(RIG_DIR / 'medusa.blend'))
    bpy.ops.object.select_all(action='DESELECT')
    rig.select_set(True)
    mesh.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.export_scene.gltf(
        filepath=str(GAME), export_format='GLB', use_selection=True,
        export_skins=True, export_animations=True, export_animation_mode='ACTIONS',
        export_force_sampling=True, export_image_format='WEBP', export_image_quality=90,
        export_leaf_bone=False, export_yup=True,
    )
    low = min((mesh.matrix_world @ v.co).z for v in mesh.data.vertices)
    high = max((mesh.matrix_world @ v.co).z for v in mesh.data.vertices)
    print(json.dumps({
        'stray_vertices_removed': stray, 'vertices': len(mesh.data.vertices),
        'triangles': sum(len(p.vertices) - 2 for p in mesh.data.polygons),
        'height_m': round(high - low, 4), 'joints': len(rig.data.bones),
        'clips': {name: duration for name, duration, _ in clips},
    }))
    if args.review:
        review(rig, actions, args.review)


main()
