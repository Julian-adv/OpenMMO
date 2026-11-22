"""Rig and animate Medusa from her Tripo source.

    blender -b -P tools/blender-scripts/rig_medusa.py -- [--review DIR]

Her upper body is a plain FK chain; below the hips a column of bones carries
the raised trunk and a second chain lies along the ground. Every clip is
written here as rotations about the rest pose's world axes.
"""
import argparse
import json
import math
import sys
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
    low = z < GROUND_Z
    radius = np.hypot(x - base[0], y - base[1])
    reach = radius[low].max()
    nodes = [np.array([base[0], base[1], 0.05])]
    edges = np.linspace(0.1, reach, TAIL_BONES + 1)
    for near, far in zip(edges[:-1], edges[1:]):
        ring = low & (radius >= near - 0.02) & (radius <= far)
        centre = points[ring & (radius > (near + far) / 2 - 0.03)].mean(axis=0)
        nodes.append(np.array([centre[0], centre[1], 0.05]))
    tip = points[low][radius[low].argmax()]
    nodes[-1] = np.array([tip[0], tip[1], 0.03])
    heights = [0.05] + TRUNK_CUTS + [1.0]
    joints = [trunk(h) for h in heights[:-1]] + [np.array([0, trunk(0.9)[1], 1.0])]
    joints[0] = nodes[0]
    return {'trunk': joints, 'arm': (arm_y, arm_z), 'tail': nodes, 'base': base, 'radius': radius,
            'tail_edges': edges}


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
        cuts = [0.03] + ARM_JOINTS + [ARM_END]
        parent = 'Chest'
        for name, near, far in zip(['Shoulder', 'Arm', 'ForeArm', 'Hand'], cuts[:-1], cuts[1:]):
            bone(side + name, (sign * near, arm_y, arm_z), (sign * far, arm_y, arm_z), parent)
            parent = side + name
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
    in_arm = smoothstep(0.07, 0.11, side) * ((z > 0.745) & (z < 0.87))
    for i, name in enumerate(TRUNK):
        dense[:, column[name]] += trunk[:, i] * (1 - in_arm)
    for prefix, mask in [('Left', x > 0), ('Right', x <= 0)]:
        for i, name in enumerate(['Shoulder', 'Arm', 'ForeArm', 'Hand']):
            dense[:, column[prefix + name]] += arm[:, i] * in_arm * mask
    radius = layout['radius']
    edges = layout['tail_edges']
    cuts = list(edges[1:-1])
    widths = [(edges[1] - edges[0]) * 0.4] * len(cuts)
    ground = chain_weights(radius, cuts, widths, TAIL_BONES)
    on_ground = smoothstep(0.06, 0.16, radius) * (z < GROUND_Z)
    dense *= 1 - on_ground[:, None]
    for i in range(TAIL_BONES):
        dense[:, column[f'Tail{i + 1}']] += ground[:, i] * on_ground

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

    def tail_pose(self, pose, yaws):
        """Lay the ground chain along `yaws`, absolute headings per bone."""
        last = 0.0
        for i, (target, rest) in enumerate(zip(yaws, self.tail_yaw)):
            delta = target - rest
            pose[f'Tail{i + 1}'] = Matrix.Rotation(delta - last, 3, 'Z')
            last = delta
        twist = yaws[0] - self.tail_yaw[0]
        # The trunk shares the turn, so the bend at the ground never wrings.
        for name, share in [('Col1', 2 / 3), ('Col2', -1 / 3), ('Col3', -1 / 3)]:
            pose[name] = Matrix.Rotation(twist * share, 3, 'Z') @ pose.get(name, Matrix.Identity(3))

    def key(self, frame, pose, lift=0.0):
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
                world_head[name] = self.rest[name].translation + Vector((0, 0, lift))
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


def arms(pose, drop, bend, spread=0.0, right_drop=None, right_bend=None, right_swing=0.0):
    """Lower the arms from the T-pose and bend the elbows forward."""
    for side, sign, d, b, swing in [('Left', 1, drop, bend, 0.0),
                                    ('Right', -1, right_drop if right_drop is not None else drop,
                                     right_bend if right_bend is not None else bend, right_swing)]:
        pose[side + 'Arm'] = rot('Z', sign * (spread + swing)) @ rot('Y', sign * d)
        pose[side + 'ForeArm'] = rot('Z', -sign * b)
        pose[side + 'Hand'] = rot('Z', -sign * b * 0.25)


def trail(wave, amplitude, speed, heading=90.0, curl=0.0):
    """Headings of a tail trailing behind, a wave running down it."""
    def at(t):
        return [math.radians(heading + curl * i) + amplitude * (0.35 + 0.65 * i / (TAIL_BONES - 1))
                * math.sin(wave * i - speed * t * math.tau) for i in range(TAIL_BONES)]
    return at


def clip_idle(t):
    pose = {}
    sway = math.sin(t / 4 * math.tau)
    pose['Col2'] = rot('Y', 1.5 * sway)
    pose['Col4'] = rot('Y', -1.0 * sway)
    pose['Chest'] = rot('X', 1.5 * math.sin(t / 2 * math.tau))
    pose['Head'] = rot('Z', 6 * math.sin(t / 4 * math.tau + 1))
    arms(pose, 62, 18)
    return pose, trail(0.9, 0.22, 0.25)(t)


def clip_combat_idle(t):
    pose = {}
    pulse = math.sin(t / 2 * math.tau)
    pose['Col3'] = rot('X', 5)
    pose['Spine'] = rot('X', 6 + 1.5 * pulse)
    pose['Head'] = rot('X', -8)
    arms(pose, 38 + 2 * pulse, 62)
    return pose, trail(1.0, 0.3, 0.5)(t)


def moving(period, lean, amplitude):
    def clip(t):
        pose = {}
        swing = math.sin(t / period * math.tau)
        pose['Col2'] = rot('Z', 5 * swing) @ rot('X', lean * 0.5)
        pose['Col4'] = rot('Z', -5 * swing)
        pose['Spine'] = rot('X', lean * 0.5) @ rot('Z', -3 * swing)
        pose['Head'] = rot('X', -lean * 0.8)
        arms(pose, 42 + 4 * swing, 55, right_drop=42 - 4 * swing)
        return pose, trail(1.1, amplitude, 1 / period)(t)
    return clip


def clip_claw(t):
    """Right-hand rake: cocked by 0.2 s, through the target at 0.36 s."""
    pose = {}
    wind = span(t, 0.0, 0.2) * (1 - span(t, 0.2, 0.38))
    follow = span(t, 0.2, 0.38) * (1 - span(t, 0.5, 1.0))
    pose['Spine'] = rot('X', 6 + 10 * follow) @ rot('Z', 22 * wind - 28 * follow)
    pose['Col3'] = rot('X', 5 + 8 * follow)
    pose['Head'] = rot('X', -8) @ rot('Z', -10 * wind + 12 * follow)
    arms(pose, 38, 62, right_drop=38 - 30 * wind + 10 * follow, right_bend=62 + 30 * wind - 50 * follow,
         right_swing=-45 * wind + 75 * follow)
    return pose, trail(1.0, 0.3, 0.5)(t)


def clip_gaze(t):
    """Rears with arms flung wide, then thrusts her face at the strike (3.0 s)."""
    pose = {}
    rear = span(t, 0.0, 0.7) * (1 - span(t, 2.6, 3.0))
    thrust = span(t, 2.6, 3.0) * (1 - span(t, 3.3, 4.0))
    tremble = math.sin(t * 38) * (span(t, 1.2, 2.6) * (1 - span(t, 3.0, 3.2)))
    pose['Col2'] = rot('X', -7 * rear + 6 * thrust)
    pose['Col4'] = rot('X', -6 * rear + 10 * thrust)
    pose['Spine'] = rot('X', 6 - 10 * rear + 16 * thrust)
    pose['Neck'] = rot('X', 8 * rear - 6 * thrust)
    pose['Head'] = rot('X', -8 + 14 * rear - 16 * thrust + 1.2 * tremble)
    open_arms = max(rear, thrust)
    arms(pose, 38 - 44 * open_arms + 14 * thrust, 62 - 50 * open_arms, spread=-18 * open_arms + 1.5 * tremble)
    return pose, trail(1.0, 0.3 + 0.25 * rear, 0.5 + 1.2 * rear)(t)


def clip_tail_sweep(t):
    """Coils the tail at her left, lashes it across behind her to her right
    at 1.6 s, and lets it trail again."""
    pose = {}
    wind = span(t, 0.0, 1.2)
    whip = span(t, 1.25, 1.95)
    settle = span(t, 1.95, 2.6)
    # She looks back over the shoulder the tail is on.
    twist = 40 * wind * (1 - whip) - 40 * whip * (1 - settle)
    pose['Col4'] = rot('Z', twist * 0.4)
    pose['Spine'] = rot('X', 6) @ rot('Z', twist * 0.4)
    pose['Head'] = rot('X', -8) @ rot('Z', twist * 0.5)
    arms(pose, 38 - 10 * whip * (1 - settle), 62, spread=-8 * whip * (1 - settle))
    yaws = []
    for i in range(TAIL_BONES):
        lag = 0.035 * i
        w = span(t - lag, 0.0, 1.2)
        s = span(t - lag, 1.25, 1.95)
        e = span(t - lag, 1.95, 2.6)
        # Headings: 90° trails behind, 0° is her left, 180° her right.
        heading = 90 - 70 * w + 140 * s - 70 * e
        loose = span(t - lag, 1.2, 1.42)
        curl = 42 * w * (1 - loose) - 7 * s * (1 - e)
        yaws.append(math.radians(heading + curl * i))
    return pose, yaws


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
    return pose, trail(1.0, 0.3 * calm + 0.05, 0.5 * calm)(t)


CLIPS = [
    ('idle1', 4.0, clip_idle),
    ('combat_idle', 2.0, clip_combat_idle),
    ('walk', 1.6, moving(1.6, 6, 0.42)),
    ('run', 1.0, moving(1.0, 14, 0.55)),
    ('claw', 1.0, clip_claw),
    ('gaze', 4.0, clip_gaze),
    ('tail_sweep', 2.6, clip_tail_sweep),
    ('dying', 1.6, clip_dying),
]


def bake(rig):
    bpy.context.scene.render.fps = FPS
    rig.animation_data_create()
    poser = Poser(rig)
    for bone in rig.pose.bones:
        bone.rotation_mode = 'QUATERNION'
    actions = {}
    for name, duration, clip in CLIPS:
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        rig.animation_data.action = action
        poser.previous = {}
        frames = round(duration * FPS)
        for frame in range(frames + 1):
            pose, yaws = clip(frame / FPS)
            poser.tail_pose(pose, yaws)
            poser.key(frame, pose)
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
    actions = bake(rig)
    if args.review:
        review(rig, actions, args.review)
    rig.animation_data.action = None
    for bone in rig.pose.bones:
        bone.matrix_basis.identity()
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
        'clips': {name: duration for name, duration, _ in CLIPS},
    }))


main()
