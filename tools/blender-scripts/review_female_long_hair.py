"""Render a rigged long straight hair on the female body through the body's own motions and count body penetration.

blender -b --python-exit-code 1 --python tools/blender-scripts/review_female_long_hair.py -- --style side --out <folder>
"""
import argparse
import json
import sys
from pathlib import Path

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
STYLES = {'side': ('tripo_long_side_v3', 'hair_female_long_side'),
          'bangs': ('tripo_long_bangs_v2', 'hair_female_long_bangs')}
parser = argparse.ArgumentParser()
parser.add_argument('--style', choices=STYLES, required=True)
parser.add_argument('--body', default=str(ROOT / 'assets/modular_human_female_01/faces/tripo_ranger_v1/base_ranger.glb'))
parser.add_argument('--motions', default=str(ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'))
parser.add_argument('--out', required=True)
parser.add_argument('--samples', type=int, default=3)
args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:])
folder, part = STYLES[args.style]
out = Path(args.out)
out.mkdir(parents=True, exist_ok=True)


def load(path):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path))
    added = set(bpy.data.objects) - before
    return (next(o for o in added if o.type == 'ARMATURE'), [o for o in added if o.type == 'MESH'])


bpy.ops.wm.read_factory_settings(use_empty=True)
donor, donor_meshes = load(args.motions)
actions = sorted(bpy.data.actions, key=lambda action: action.name)
for mesh in donor_meshes:
    bpy.data.objects.remove(mesh)
body_rig, body_meshes = load(args.body)
for action in list(bpy.data.actions):
    if action not in actions:
        bpy.data.actions.remove(action)
hair_rig, hair_meshes = load(ROOT / 'assets/modular_human_female_01/hair' / folder / 'fitted_v1' / f'{part}.glb')
rigs = [body_rig, hair_rig]
scene = bpy.context.scene
scene.render.engine = 'BLENDER_EEVEE'
scene.render.resolution_x, scene.render.resolution_y = 420, 560
scene.view_settings.view_transform = 'AgX'
scene.world = bpy.data.worlds.new('Review')
scene.world.use_nodes = True
scene.world.node_tree.nodes['Background'].inputs[0].default_value = (.5, .5, .55, 1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value = 1
sun = bpy.data.objects.new('key', bpy.data.lights.new('key', 'SUN'))
sun.data.energy = 2.5
scene.collection.objects.link(sun)
camera = bpy.data.objects.new('camera', bpy.data.cameras.new('camera'))
camera.data.type, camera.data.ortho_scale = 'ORTHO', 1.0
scene.collection.objects.link(camera)
scene.camera = camera


def world_points(obj, depsgraph):
    mesh = obj.evaluated_get(depsgraph).to_mesh()
    return [obj.matrix_world @ v.co for v in mesh.vertices], [p.vertices[:] for p in mesh.polygons]


def inside(tree, point):
    # Odd number of crossings along two directions.
    votes = 0
    for direction in (Vector((1, 0, 0)), Vector((0, 0, 1))):
        crossings, origin, left = 0, point, 3.0
        while left > 0:
            hit = tree.ray_cast(origin, direction, left)
            if hit[0] is None:
                break
            crossings += 1
            left -= hit[3] + 1e-5
            origin = hit[0] + direction * 1e-5
        votes += crossings % 2
    return votes == 2


report = {}
rest_inside = None
for action in [None] + actions:
    name = action.name if action else 'rest'
    for rig in rigs:
        rig.animation_data_create()
        rig.animation_data.action = action
        if action and hasattr(rig.animation_data, 'action_slot') and action.slots:
            rig.animation_data.action_slot = action.slots[0]
        rig.data.pose_position = 'POSE' if action else 'REST'
    start, end = (action.frame_range if action else (0, 0))
    frames = [start + (end - start) * (i + .5) / args.samples for i in range(args.samples)] if action else [0]
    worst = 0
    for number, frame in enumerate(frames):
        scene.frame_set(int(frame), subframe=frame % 1)
        depsgraph = bpy.context.evaluated_depsgraph_get()
        points, polygons = [], []
        for mesh in body_meshes:
            p, f = world_points(mesh, depsgraph)
            polygons += [[len(points) + i for i in face] for face in f]
            points += p
        tree = BVHTree.FromPolygons(points, polygons)
        hair_points = [p for mesh in hair_meshes for p in world_points(mesh, depsgraph)[0]]
        flags = [inside(tree, p) and (tree.find_nearest(p)[0] - p).length > .005 for p in hair_points]
        if rest_inside is None:
            rest_inside = flags
        worst = max(worst, sum(1 for now, rest in zip(flags, rest_inside) if now and not rest))
        head = body_rig.matrix_world @ body_rig.pose.bones['Head'].head if action else Vector((0, 0, 1.62))
        centre = Vector((head.x, head.y, head.z - .12))
        if number != len(frames) // 2 and action:
            continue
        for view, offset in (('front', (0, -4, .3)), ('side', (4, 0, .3)), ('back', (0, 4, .3))):
            camera.location = centre + Vector(offset)
            camera.rotation_euler = (centre - camera.location).to_track_quat('-Z', 'Y').to_euler()
            sun.rotation_euler = (centre - (camera.location + Vector((1, 0, 2)))).to_track_quat('-Z', 'Y').to_euler()
            scene.render.filepath = str(out / f'{name}-{view}.png')
            bpy.ops.render.render(write_still=True)
    report[name] = {'frames': len(frames), 'vertices_newly_inside_body_over_5mm': worst}
report['rest']['vertices_inside_body_over_5mm'] = sum(rest_inside)
report['hair_vertices'] = len(rest_inside)
(out / 'motion-review.json').write_text(json.dumps(report, indent=2) + '\n')
print('REVIEW', json.dumps(report))
