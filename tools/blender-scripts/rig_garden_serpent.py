"""Rig and animate the Stonescale Serpent from its Tripo source.

    blender -b -P tools/blender-scripts/rig_garden_serpent.py -- [--review DIR]

"""
import argparse
import json
import math
import sys
from pathlib import Path

import bpy
import numpy as np
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/monsters/garden_serpent/tripo_v1/garden_serpent.glb'
RIG_DIR = ROOT / 'assets/monsters/garden_serpent/rig_v1'
GAME = ROOT / 'client/public/models/monsters/garden_serpent.glb'
LENGTH = 4.0
ORIGIN_BEHIND_HEAD = 0.8
FPS = 30
TEXTURE = 1024
ROOT_SHARE = ORIGIN_BEHIND_HEAD / LENGTH
CUTS = [0.0, 0.07, 0.13, ROOT_SHARE, 0.26, 0.32, 0.40, 0.48, 0.56, 0.64, 0.72, 0.80, 0.88, 1.0]
BLEND = 0.025
FRONT = ['Head', 'Neck2', 'Neck1']
BACK = [f'Tail{i + 1}' for i in range(len(CUTS) - 1 - len(FRONT))]
SEGMENTS = FRONT + BACK
NECK = len(FRONT)


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


def load_mesh():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(SOURCE))
    mesh = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
    for other in [o for o in bpy.context.scene.objects if o is not mesh]:
        bpy.data.objects.remove(other)
    bpy.context.view_layer.objects.active = mesh
    mesh.select_set(True)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    mesh.name = mesh.data.name = 'StonescaleSerpent'
    co = np.array([v.co for v in mesh.data.vertices])
    # Rotate the imported normals with the mesh.
    scale = LENGTH / np.ptp(co[:, 0])
    mesh.rotation_mode = 'XYZ'
    mesh.rotation_euler = (0, 0, math.pi / 2)
    mesh.scale = (scale, scale, scale)
    mesh.location = (0, -co[:, 0].min() * scale - ORIGIN_BEHIND_HEAD, -co[:, 2].min() * scale)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    for image in bpy.data.images:
        if image.size[0] > TEXTURE:
            image.scale(TEXTURE, TEXTURE)
    return mesh


def centreline(points):
    """Centre of the body at each cut, from the snout back."""
    snout = -ORIGIN_BEHIND_HEAD
    stations = []
    for share in CUTS:
        y = snout + share * LENGTH
        near = points[np.abs(points[:, 1] - y) < 0.06]
        if len(near) < 3:
            near = points[np.argsort(np.abs(points[:, 1] - y))[:12]]
        stations.append((float(near[:, 0].mean()), y, float((near[:, 2].min() + near[:, 2].max()) / 2)))
    return stations


def build_armature(stations):
    data = bpy.data.armatures.new('SerpentRig')
    rig = bpy.data.objects.new('SerpentRig', data)
    bpy.context.scene.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')
    root = data.edit_bones.new('Root')
    root.head, root.tail = Vector((0, 0, 0)), Vector((0, 0, 0.1))
    pivot = CUTS.index(ROOT_SHARE)
    parent = root
    for k in range(NECK - 1, -1, -1):
        bone = data.edit_bones.new(SEGMENTS[k])
        bone.head, bone.tail = Vector(stations[k + 1]), Vector(stations[k])
        bone.parent = parent
        parent = bone
    parent = root
    for k in range(pivot, len(SEGMENTS)):
        bone = data.edit_bones.new(SEGMENTS[k])
        bone.head, bone.tail = Vector(stations[k]), Vector(stations[k + 1])
        bone.parent = parent
        parent = bone
    bpy.ops.object.mode_set(mode='OBJECT')
    return rig


def skin(mesh, rig):
    points = np.array([v.co for v in mesh.data.vertices])
    share = (points[:, 1] + ORIGIN_BEHIND_HEAD) / LENGTH
    weights = np.zeros((len(points), len(SEGMENTS)))
    for k in range(len(SEGMENTS)):
        lo, hi = CUTS[k], CUTS[k + 1]
        past = smoothstep(lo - BLEND, lo + BLEND, share) if lo > 0 else np.ones(len(points))
        ahead = smoothstep(hi - BLEND, hi + BLEND, share) if hi < 1 else np.zeros(len(points))
        weights[:, k] = past * (1 - ahead)
    weights /= weights.sum(axis=1, keepdims=True)
    groups = [mesh.vertex_groups.new(name=n) for n in SEGMENTS]
    for vertex, row in enumerate(weights):
        for k in np.flatnonzero(row > 1e-4):
            groups[k].add([vertex], float(row[k]), 'REPLACE')
    modifier = mesh.modifiers.new('Armature', 'ARMATURE')
    modifier.object = rig
    mesh.parent = rig


class Poser:
    """Forward kinematics over rotations given in the rest pose's world axes."""

    def __init__(self, rig):
        self.rig = rig
        self.order = [b.name for b in rig.data.bones]
        self.rest = {b.name: b.matrix_local.copy() for b in rig.data.bones}
        self.parent = {b.name: b.parent.name if b.parent else None for b in rig.data.bones}
        self.previous = {}

    def key(self, frame, pose, root_turn, offset, anchor=None):
        world_rot, world_head, matrices = {}, {}, {}
        for name in self.order:
            parent = self.parent[name]
            delta = pose.get(name, Matrix.Identity(3))
            if parent:
                world_rot[name] = world_rot[parent] @ delta
                rest_offset = self.rest[name].translation - self.rest[parent].translation
                world_head[name] = world_head[parent] + world_rot[parent] @ rest_offset
            else:
                world_rot[name] = root_turn @ delta
                world_head[name] = self.rest[name].translation.copy()
        if anchor:
            if frame == 0:
                self.anchor_start = world_head[anchor].copy()
            offset += self.anchor_start - world_head[anchor]
        for name in self.order:
            matrices[name] = Matrix.Translation(world_head[name] + offset) @ (world_rot[name] @ self.rest[name].to_3x3()).to_4x4()
        for name in self.order:
            bone = self.rig.pose.bones[name]
            parent = self.parent[name]
            if parent:
                basis = (self.rest[parent].inverted() @ self.rest[name]).inverted() @ (matrices[parent].inverted() @ matrices[name])
            else:
                basis = self.rest[name].inverted() @ matrices[name]
            bone.matrix_basis = basis
            q = bone.rotation_quaternion
            if name in self.previous and q.dot(self.previous[name]) < 0:
                q.negate()
            self.previous[name] = q.copy()
            bone.keyframe_insert('rotation_quaternion', frame=frame, group=name)
            if not parent:
                bone.keyframe_insert('location', frame=frame, group=name)


def rot(axis, degrees):
    return Matrix.Rotation(math.radians(degrees), 3, axis)


def ease(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def span(t, start, end):
    return ease((t - start) / (end - start))


def heading(yaw, pitch=0.0):
    return rot('Z', yaw) @ rot('X', pitch)


def to_pose(yaws, pitches, rig):
    """Per-bone rotations, relative to the parent, for absolute headings."""
    absolute = {name: heading(yaws[k], pitches[k]) for k, name in enumerate(SEGMENTS)}
    pose = {}
    for name in SEGMENTS:
        parent = rig.data.bones[name].parent.name
        pose[name] = (absolute[parent].transposed() if parent in absolute else Matrix.Identity(3)) @ absolute[name]
    return pose


def wave(t, period, amplitude):
    """A sideways wave running from the head to the tail; the head stays on course."""
    yaws = []
    for k in range(len(SEGMENTS)):
        reach = amplitude * min(1.0, k / 4)
        yaws.append(reach * math.cos(2 * math.pi * (t / period - k / 6.0)))
    return yaws


def lifted(pitch, head=0.0):
    """Neck pitch spread over the head-side segments; +pitch lowers it."""
    pitches = [0.0] * len(SEGMENTS)
    for k in range(NECK):
        pitches[k] = pitch * (NECK - k) / NECK
    pitches[0] += head
    return pitches


def idle(t, period, sway, lift):
    s = math.sin(2 * math.pi * t / period)
    yaws = wave(t, period, 1.5)
    for k in range(NECK):
        yaws[k] += sway * math.sin(2 * math.pi * t / period + 0.7) * (NECK - k) / NECK
    return yaws, lifted(-lift + 2 * s, -s)


def slither(t, period, wavelength, amplitude):
    phase = math.tau * t / period
    lateral = []
    for share in CUTS:
        depth = share * LENGTH
        width = 0.12 + 0.88 * ease(depth / 1.55)
        lateral.append(amplitude * width * math.sin(phase - math.tau * depth / wavelength))
    yaws = []
    for k, (near, far) in enumerate(zip(lateral, lateral[1:])):
        length = LENGTH * (CUTS[k + 1] - CUTS[k])
        yaws.append(math.degrees(math.asin((near - far) / length)))
    return yaws, lifted(-3), lateral[NECK]


def bite(t):
    u = min(t / 0.34, 1.0)
    gather = u ** 3 * (10 - 15 * u + 6 * u ** 2)
    release = span(t, 0.34, 0.50)
    load = gather * (1 - release)
    thrust = release * (1 - span(t, 0.50, 0.98))
    yaws, pitches = idle(0, 2, 6, 8)
    coil = [0, -35, -35, 26, 52, 28, -25, -38, -10, 5, 9, 5, 0]
    rise = [2, -22, -34, -19, -7]
    snap = [6, 19, 16]
    for k in range(len(SEGMENTS)):
        yaws[k] = yaws[k] * (1 - thrust) + coil[k] * load
        if k < len(rise):
            pitches[k] += rise[k] * load
        if k < len(snap):
            pitches[k] += (snap[k] - pitches[k]) * thrust
    for k in range(NECK + 5, len(SEGMENTS)):
        delay = (k - NECK - 5) * 0.055
        pulse = span(t - delay, 0.38, 0.55) - span(t - delay, 0.55, 0.93)
        yaws[k] += 17 * pulse
    brace = gather ** 2 * (1 - span(t, 0.55, 1.12))
    return yaws, pitches, -0.28 * brace


def hit(t):
    jolt = span(t, 0.0, 0.12) * (1 - span(t, 0.2, 0.6))
    yaws = [8 * jolt * (NECK - k) / NECK if k < NECK else 0.0 for k in range(len(SEGMENTS))]
    return yaws, lifted(-6 - 18 * jolt, 6 * jolt)


CLIPS = [
    ('idle1', 4.0), ('combat_idle', 2.0), ('walk', 1.2), ('run', 0.7),
    ('bite', 1.2), ('hit', 0.6), ('dying', 1.5),
]


def bake(rig):
    bpy.context.scene.render.fps = FPS
    poser = Poser(rig)
    rig.animation_data_create()
    rig.rotation_mode = 'XYZ'
    for bone in rig.pose.bones:
        bone.rotation_mode = 'QUATERNION'
    actions = {}
    for clip, seconds in CLIPS:
        action = bpy.data.actions.new(clip)
        action.use_fake_user = True
        rig.animation_data.action = action
        poser.previous = {}
        frames = round(seconds * FPS)
        for f in range(frames + 1):
            t = f / FPS
            root_turn, offset, anchor = Matrix.Identity(3), Vector((0, 0, 0)), None
            if clip == 'idle1':
                yaws, pitches = idle(t, seconds, 4, 0)
            elif clip == 'combat_idle':
                yaws, pitches = idle(t, seconds, 6, 8)
            elif clip == 'walk':
                yaws, pitches, offset.x = slither(t, seconds, 2.8, 0.25)
            elif clip == 'run':
                yaws, pitches, offset.x = slither(t, seconds, 4.5, 0.43)
            elif clip == 'bite':
                yaws, pitches, offset.y = bite(t)
                anchor = 'Tail6'
            elif clip == 'hit':
                yaws, pitches = hit(t)
            else:
                fall = span(t, 0.1, 1.0)
                yaws = [v * fall for v in wave(0.3, 1.0, 10)]
                pitches = lifted(30 * fall, 20 * fall)
                root_turn = rot('Y', 75 * fall)
                offset.z = -0.05 * fall
            pose = to_pose(yaws, pitches, rig)
            poser.key(f, pose, root_turn, offset, anchor)
        actions[clip] = action
    return actions


def review(rig, actions, out):
    out.mkdir(parents=True, exist_ok=True)
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_EEVEE'
    scene.render.resolution_x = scene.render.resolution_y = 420
    world = bpy.data.worlds.new('w')
    world.color = (0.35, 0.35, 0.38)
    scene.world = world
    sun = bpy.data.objects.new('sun', bpy.data.lights.new('sun', 'SUN'))
    sun.data.energy = 3
    sun.rotation_euler = (0.8, 0.2, 0.6)
    scene.collection.objects.link(sun)
    cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
    cam.data.type = 'ORTHO'
    cam.data.ortho_scale = 5.0
    scene.collection.objects.link(cam)
    scene.camera = cam
    centre = Vector((0, 1.0, 0.4))
    cam.location = centre + Vector((5, -3, 4))
    cam.rotation_euler = (centre - cam.location).to_track_quat('-Z', 'Y').to_euler()
    for clip, seconds in CLIPS:
        rig.animation_data.action = actions[clip]
        for share in (0.0, 0.45):
            scene.frame_set(round(seconds * FPS * share))
            scene.render.filepath = str(out / f'{clip}-{int(share * 100):02d}.png')
            bpy.ops.render.render(write_still=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--review', type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    mesh = load_mesh()
    points = np.array([v.co for v in mesh.data.vertices])
    rig = build_armature(centreline(points))
    skin(mesh, rig)
    actions = bake(rig)
    bpy.context.scene.frame_start = 0
    bpy.context.scene.frame_end = round(max(dict(CLIPS).values()) * FPS)
    rig.animation_data.action = None
    for bone in rig.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    RIG_DIR.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(RIG_DIR / 'garden_serpent.blend'))
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
    print(json.dumps({
        'vertices': len(mesh.data.vertices),
        'triangles': sum(len(p.vertices) - 2 for p in mesh.data.polygons),
        'joints': len(rig.data.bones), 'bytes': GAME.stat().st_size,
        'clips': dict(CLIPS),
    }))
    if args.review:
        review(rig, actions, args.review)


main()
