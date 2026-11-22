"""Rig and animate the Floating Eye from its Tripo source.

    blender -b -P tools/blender-scripts/rig_floating_eye.py -- [--review DIR]

The eyeball hangs from a root on the ground. Its eyelids were generated shut
on a blue mannequin of the eye (tools/split-floating-eye-lids.py); the upper
and the lower swing open on bones through the ball's centre and slide under a
fixed hood over its back. The tendrils sway on one short chain. Every bone
points along +Y with no roll, so pose values are plain world-axis rotations
and offsets.
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
SOURCE = ROOT / 'assets/monsters/floating_eye/tripo_v1/model_url.glb'
LID_SOURCE = ROOT / 'assets/monsters/floating_eye/eyelids/fitted_v1/lids-split.glb'
HOOD = ROOT / 'assets/monsters/floating_eye/eyelids/fitted_v1/hood-split.glb'
RIG_DIR = ROOT / 'assets/monsters/floating_eye/rig_v1'
GAME = ROOT / 'client/public/models/monsters/floating_eye.glb'
FPS = 30
TEXTURE = 512
EYE_DIAMETER = 0.8
HOVER = 1.3  # eyeball centre above the ground
LID_OPEN = 62  # degrees each lid swings clear of the iris
LIDS = ['LidUpper', 'LidLower']
LID_LIFT = 1.02  # the lids ride this far out from the ball's widest point
LID_THICKNESS = 0.05
HOOD_GAP = 0.008  # the fixed hood behind rides this far outside the lids
HOOD_THICKNESS = 0.02
TENDRIL_BONES = 3


def import_mesh(path, name):
    before = set(bpy.context.scene.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
    added = [o for o in bpy.context.scene.objects if o not in before]
    mesh = next(o for o in added if o.type == 'MESH')
    for other in added:
        if other is not mesh:
            bpy.data.objects.remove(other)
    bpy.ops.object.select_all(action='DESELECT')
    bpy.context.view_layer.objects.active = mesh
    mesh.select_set(True)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    mesh.name = mesh.data.name = name
    points = np.zeros(len(mesh.data.vertices) * 3)
    mesh.data.vertices.foreach_get('co', points)
    points = points.reshape(-1, 3)
    # The API models look along +X; the game's front is -Y.
    return mesh, np.stack([points[:, 1], -points[:, 0], points[:, 2]], axis=1)


def place(mesh, points):
    mesh.data.vertices.foreach_set('co', points.reshape(-1))
    mesh.data.update()


def load_meshes():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    mesh, points = import_mesh(SOURCE, 'FloatingEye')
    top = points[:, 2].max()
    radius = np.ptp(points[:, 0]) / 2
    ball = points[points[:, 2] > top - radius]
    centre = np.array([ball[:, 0].mean(), ball[:, 1].mean(), top - radius])
    scale = EYE_DIAMETER / (2 * radius)
    hover = np.array([0, 0, HOVER])
    points = (points - centre) * scale + hover
    place(mesh, points)
    reach = np.linalg.norm(points[points[:, 2] > HOVER - EYE_DIAMETER * 0.4] - hover, axis=1).max() * LID_LIFT

    def onto_sphere(source_points, sphere):
        offsets = (source_points - centre) * scale
        return offsets / np.linalg.norm(offsets, axis=1, keepdims=True) * sphere + hover

    # Same frame as the eye. The lids go onto one sphere clearing the cornea
    # and are thickened inward; the hood rides just outside them.
    shut, lid_points = import_mesh(LID_SOURCE, 'EyeLids')
    place(shut, onto_sphere(lid_points, reach))
    split_lids(shut)
    hood, hood_points = import_mesh(HOOD, 'EyeHood')
    place(hood, onto_sphere(hood_points, reach + HOOD_GAP))
    weld(hood)
    hood.vertex_groups.new(name='Body').add(range(len(hood.data.vertices)), 1.0, 'REPLACE')
    for sheet, thickness, offset in [(shut, LID_THICKNESS, -1), (hood, HOOD_THICKNESS, 1)]:
        flesh = sheet.modifiers.new('Flesh', 'SOLIDIFY')
        flesh.thickness = thickness
        flesh.offset = offset
        bpy.context.view_layer.objects.active = sheet
        bpy.ops.object.modifier_apply(modifier=flesh.name)
    # One mesh and one material for both: they share the generated texture.
    hood.data.materials[0] = shut.data.materials[0]
    bpy.ops.object.select_all(action='DESELECT')
    hood.select_set(True)
    shut.select_set(True)
    bpy.context.view_layer.objects.active = shut
    bpy.ops.object.join()
    for image in bpy.data.images:
        if image.size[0] > TEXTURE:
            image.scale(TEXTURE, TEXTURE)
    return mesh, points, shut


def weld(sheet):
    """Texture seams leave a generated sheet in pieces; join it so it shades as one."""
    bm = bmesh.new()
    bm.from_mesh(sheet.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    for face in bm.faces:
        face.smooth = True
    bm.to_mesh(sheet.data)
    bm.free()


def split_lids(shut):
    """Part the shut lids along the seam at the ball's middle, each weighted to its bone."""
    weld(shut)
    bm = bmesh.new()
    bm.from_mesh(shut.data)

    def lower(face):
        return face.calc_center_median().z < HOVER

    bmesh.ops.split(bm, geom=[f for f in bm.faces if lower(f)])
    bm.verts.index_update()
    members = {name: sorted({v.index for f in bm.faces if lower(f) == below for v in f.verts})
               for name, below in zip(LIDS, (False, True))}
    bm.to_mesh(shut.data)
    bm.free()
    for name, vertices in members.items():
        shut.vertex_groups.new(name=name).add(vertices, 1.0, 'REPLACE')


def build_armature(points):
    data = bpy.data.armatures.new('FloatingEyeRig')
    rig = bpy.data.objects.new('FloatingEyeRig', data)
    bpy.context.scene.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode='EDIT')

    def bone(name, head, parent=None):
        b = data.edit_bones.new(name)
        b.head = Vector(head)
        b.tail = b.head + Vector((0, 0.12, 0))
        b.roll = 0
        b.parent = data.edit_bones[parent] if parent else None

    bone('Root', (0, 0, 0))
    bone('Body', (0, 0, HOVER), 'Root')
    for name in LIDS:
        bone(name, (0, 0, HOVER), 'Body')
    bottom = HOVER - EYE_DIAMETER / 2
    low = points[:, 2].min()
    cuts = np.linspace(bottom, low, TENDRIL_BONES + 1)
    for i in range(TENDRIL_BONES):
        bone(f'Tendril{i + 1}', (0, 0, cuts[i]), f'Tendril{i}' if i else 'Body')
    bpy.ops.object.mode_set(mode='OBJECT')
    return rig, cuts


def skin(mesh, lids, rig, points, cuts):
    groups = {name: mesh.vertex_groups.new(name=name)
              for name in ['Body', *(f'Tendril{i + 1}' for i in range(TENDRIL_BONES))]}
    z = points[:, 2]
    # Each tendril bone takes over across the length of the one above.
    depth = np.clip((cuts[0] - z) / (cuts[0] - cuts[-1]), 0, 1) * TENDRIL_BONES
    for vertex in range(len(points)):
        d = depth[vertex]
        if d <= 0:
            groups['Body'].add([vertex], 1.0, 'REPLACE')
            continue
        lower = min(int(d), TENDRIL_BONES - 1)
        part = d - lower
        above = 'Body' if lower == 0 else f'Tendril{lower}'
        groups[above].add([vertex], float(1 - part), 'REPLACE')
        groups[f'Tendril{lower + 1}'].add([vertex], float(part), 'REPLACE')
    for obj in [mesh, lids]:
        obj.modifiers.new('Armature', 'ARMATURE').object = rig
        obj.parent = rig


def ease(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def span(t, start, end):
    return ease((t - start) / (end - start))


def blink(t, at):
    """How shut the lids are (0-1) for a blink starting at `at`."""
    return span(t, at, at + 0.1) * (1 - span(t, at + 0.16, at + 0.34))


def drifting(lean, trail, blink_at):
    def clip(t, period):
        phase = t / period * math.tau
        return {
            'lift': 0.06 * math.sin(phase),
            'body': (lean + 2 * math.sin(phase + 1), 3 * math.sin(phase + 3), 4 * math.sin(phase + 2)),
            'shut': blink(t, blink_at),
            'tendrils': [trail + 7 * math.sin(phase - 0.9 * i) for i in range(TENDRIL_BONES)],
        }
    return clip


def clip_dying(t):
    """Shuts its eye, drops and rolls onto its side."""
    fall = span(t, 0.15, 0.7) ** 2
    rest = HOVER - EYE_DIAMETER / 2
    return {
        'lift': -rest * fall + 0.05 * math.sin(span(t, 0.7, 1.0) * math.pi),
        'body': (0, 55 * span(t, 0.5, 1.2), 0),
        'shut': span(t, 0.0, 0.25),
        'tendrils': [60 * span(t, 0.4, 1.1)] * TENDRIL_BONES,
    }


def clip_stare(t):
    """One slow blink at whoever struck it; the hold lands as it reopens."""
    swell = math.sin(span(t, 0.0, 0.7) * math.pi)
    return {
        'lift': 0.04 * swell,
        'body': (-6 * swell, 0, 0),
        'shut': span(t, 0.0, 0.2) * (1 - span(t, 0.35, 0.6)),
        'tendrils': [10 * swell] * TENDRIL_BONES,
    }


CLIPS = [
    ('idle', 4.0, drifting(0, 0, 1.6)),
    ('drift', 2.0, drifting(8, 16, 0.9)),
    ('stare', 0.7, lambda t, _: clip_stare(t)),
    ('dying', 1.2, lambda t, _: clip_dying(t)),
]


def bake(rig):
    bpy.context.scene.render.fps = FPS
    rig.animation_data_create()
    for bone in rig.pose.bones:
        bone.rotation_mode = 'XYZ'
    actions = {}
    for name, duration, clip in CLIPS:
        action = bpy.data.actions.new(name)
        action.use_fake_user = True
        rig.animation_data.action = action
        for frame in range(round(duration * FPS) + 1):
            pose = clip(frame / FPS, duration)
            bones = rig.pose.bones
            bones['Body'].location = (0, 0, pose['lift'])
            bones['Body'].rotation_euler = [math.radians(a) for a in pose['body']]
            opened = math.radians(LID_OPEN * (1 - pose['shut']))
            for lid, sign in zip(LIDS, (-1, 1)):
                bones[lid].rotation_euler = (sign * opened, 0, 0)
            for i, angle in enumerate(pose['tendrils']):
                bones[f'Tendril{i + 1}'].rotation_euler = (-math.radians(angle) / TENDRIL_BONES, 0, 0)
            for bone in bones:
                bone.keyframe_insert('rotation_euler', frame=frame, group=bone.name)
            bones['Body'].keyframe_insert('location', frame=frame, group='Body')
        if action.slots:
            rig.animation_data.action_slot = action.slots[0]
        actions[name] = action
    return actions


def review(rig, actions, directory):
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_EEVEE'
    scene.render.resolution_x = scene.render.resolution_y = 560
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
    camera.data.ortho_scale = 2.6
    target = Vector((0, 0, 0.9))
    directory.mkdir(parents=True, exist_ok=True)
    views = {'front': (0.25, -1, 0.15), 'side': (1, -0.1, 0.1), 'back': (0.7, 1, 0.5), 'game': (1, -1, 1.15)}
    shots = [('idle', t, view) for t in (0.0, 1.7) for view in views] + [('dying', 1.2, 'front')]
    for name, t, view in shots:
        rig.animation_data.action = actions[name]
        if actions[name].slots:
            rig.animation_data.action_slot = actions[name].slots[0]
        scene.frame_set(round(t * FPS))
        camera.location = target + Vector(views[view]).normalized() * 20
        camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
        scene.render.filepath = str(directory / f'{name}_{t:.2f}_{view}.png')
        bpy.ops.render.render(write_still=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--review', type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    mesh, points, lids = load_meshes()
    rig, cuts = build_armature(points)
    skin(mesh, lids, rig, points, cuts)
    actions = bake(rig)
    if args.review:
        review(rig, actions, args.review)
    rig.animation_data.action = None
    for bone in rig.pose.bones:
        bone.matrix_basis = Matrix.Identity(4)
    RIG_DIR.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(RIG_DIR / 'floating_eye.blend'))
    bpy.ops.object.select_all(action='DESELECT')
    for obj in [rig, mesh, lids]:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.export_scene.gltf(
        filepath=str(GAME), export_format='GLB', use_selection=True,
        export_skins=True, export_animations=True, export_animation_mode='ACTIONS',
        export_force_sampling=True, export_image_format='WEBP', export_image_quality=90,
        export_leaf_bone=False, export_yup=True,
    )
    print(json.dumps({
        'vertices': len(mesh.data.vertices) + len(lids.data.vertices),
        'triangles': sum(len(p.vertices) - 2 for o in [mesh, lids] for p in o.data.polygons),
        'height_m': round(max((o.matrix_world @ v.co).z for o in [mesh, lids] for v in o.data.vertices), 3), 'joints': len(rig.data.bones),
        'clips': {name: duration for name, duration, _ in CLIPS},
    }))


main()
