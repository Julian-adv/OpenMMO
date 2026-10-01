"""Build provisional outfit interface contours from the canonical rest mesh."""
import argparse
import hashlib
import json
import sys
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]


def game(vector):
    return Vector((vector.x, vector.z, -vector.y))


def blender(vector):
    return Vector((vector[0], -vector[2], vector[1]))


def section(triangles, center, normal, side):
    edges = set()
    points = {}
    for triangle in triangles:
        if side and sum(v.x for v in triangle) * side <= 0:
            continue
        distances = [(v - center).dot(normal) for v in triangle]
        hits = []
        for i, j in ((0, 1), (1, 2), (2, 0)):
            if distances[i] * distances[j] < 0:
                hits.append(triangle[i].lerp(triangle[j], distances[i] / (distances[i] - distances[j])))
        if len(hits) != 2:
            continue
        keys = [tuple(round(v, 5) for v in hit) for hit in hits]
        if keys[0] == keys[1]:
            continue
        points.update(zip(keys, hits))
        edges.add(tuple(sorted(keys)))
    adjacent = {}
    for a, b in edges:
        adjacent.setdefault(a, set()).add(b)
        adjacent.setdefault(b, set()).add(a)
    loops = []
    while adjacent:
        start = next(iter(adjacent))
        component, pending = set(), [start]
        while pending:
            point = pending.pop()
            if point in component:
                continue
            component.add(point)
            pending.extend(adjacent[point] - component)
        if all(len(adjacent[p]) == 2 for p in component):
            ordered, previous, current = [], None, start
            while True:
                ordered.append(points[current])
                following = next(p for p in sorted(adjacent[current]) if p != previous)
                previous, current = current, following
                if current == start:
                    break
            loops.append(ordered)
        for p in component:
            del adjacent[p]
    if not loops:
        raise ValueError('No closed contour at requested section')
    return min(loops, key=lambda loop: (sum(loop, Vector()) / len(loop) - center).length)


def draw(name, points, collection):
    curve = bpy.data.curves.new(name, 'CURVE')
    curve.dimensions = '3D'
    curve.bevel_depth = .001
    spline = curve.splines.new('POLY')
    spline.points.add(len(points) - 1)
    for point, coordinate in zip(spline.points, points):
        point.co = (*blender(coordinate), 1)
    spline.use_cyclic_u = True
    obj = bpy.data.objects.new(name, curve)
    collection.objects.link(obj)
    obj['status'] = 'provisional; not animation-validated'
    palette = {'neck': (.9, .05, .5, 1), 'waist': (1, .7, .02, 1),
               'glove_short': (.03, .3, 1, 1), 'glove_long': (1, .03, .07, 1),
               'boot_thigh': (.01, .7, .9, 1), 'boot_calf': (.5, .12, .9, 1),
               'shoe_ankle': (.03, .8, .15, 1)}
    color = next(value for key, value in palette.items() if name.startswith(key))
    material = bpy.data.materials.new(name)
    material.diffuse_color = color
    curve.materials.append(material)
    return obj


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', default='assets/modular_human_male_01/parts/fitted/base.glb')
    parser.add_argument('--output')
    parser.add_argument('--version', default='v1')
    parser.add_argument('--preview', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else [])
    base = ROOT / args.base
    output = ROOT / args.output if args.output else base.parent.parent / 'interfaces' / args.version
    base_hash = hashlib.sha256(base.read_bytes()).hexdigest()
    previous = output / 'interfaces.json'
    if previous.exists():
        old = json.loads(previous.read_text())
        if old['base_sha256'] != base_hash or old['version'] != args.version:
            raise ValueError('Reference changed; use a new version and output directory')
    output.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(base))
    rig = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
    rig.data.pose_position = 'REST'
    bones = {bone.name: game(rig.matrix_world @ bone.head_local) for bone in rig.data.bones}
    triangles = []
    for obj in list(bpy.context.scene.objects):
        if obj.type != 'MESH':
            continue
        if obj.name.startswith('body_boot_ankles'):
            obj.hide_render = True
            obj.hide_set(True)
            continue
        obj.data.calc_loop_triangles()
        vertices = [game(obj.matrix_world @ v.co) for v in obj.data.vertices]
        triangles.extend([[vertices[i] for i in tri.vertices] for tri in obj.data.loop_triangles])
    specs = [('neck_base', Vector((0, 1.58, bones['Neck'].z)), Vector((0, 1, 0)), 0, .02),
             ('waist', Vector((0, 1.12, bones['Hips'].z)), Vector((0, 1, 0)), 0, .03)]
    for side, sign in [('Left', 1), ('Right', -1)]:
        elbow, wrist = bones[side + 'ForeArm'], bones[side + 'Hand']
        axis = (elbow - wrist).normalized()
        specs.extend([
            ('glove_short_' + side, wrist + axis * .04, axis, sign, .025),
            ('glove_long_' + side, elbow - axis * .065, axis, sign, .035),
        ])
        hip, knee, ankle = [bones[side + b] for b in ('UpLeg', 'Leg', 'Foot')]
        for name, height, start, end, depth in [
            ('boot_thigh', .88, knee, hip, .04),
            ('boot_calf', .49, ankle, knee, .04),
            ('shoe_ankle', .235, ankle, knee, .035),
        ]:
            axis = (end - start).normalized()
            center = start + (end - start) * ((height - start.y) / (end.y - start.y))
            specs.append((name + '_' + side, center, axis, sign, depth))
    collection = bpy.data.collections.new(f'Outfit interfaces {args.version} — provisional')
    bpy.context.scene.collection.children.link(collection)
    report = dict(version=args.version, status='provisional; rest-mesh contours only; no compatibility certification',
                  base=args.base, base_sha256=base_hash,
                  rig_id='human_male_01_mixamo_candidate_v2', coordinates='meters, Y up, +Z forward',
                  interfaces=[])
    for name, center, normal, side, depth in specs:
        contours = {}
        for suffix, offset in [('center', 0), ('band_minus', -depth / 2), ('band_plus', depth / 2)]:
            loop = section(triangles, center + normal * offset, normal, side)
            draw(name + '_' + suffix, loop, collection)
            contours[suffix] = dict(points=[list(v) for v in loop],
                                    perimeter_m=sum((loop[i] - loop[i - 1]).length for i in range(len(loop))))
        report['interfaces'].append(dict(name=name, center=list(center), normal=list(normal),
                                         candidate_band_depth_m=depth, garment_clearance_m=None,
                                         contours=contours))
        print(name, round(contours['center']['perimeter_m'], 4), flush=True)
    (output / 'interfaces.json').write_text(json.dumps(report, indent=2) + '\n')
    bpy.ops.file.pack_all()
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(output / 'outfit-reference.blend'))
    if args.preview:
        scene = bpy.context.scene
        scene.render.engine = 'BLENDER_WORKBENCH'
        scene.display.shading.light = 'STUDIO'
        scene.display.shading.color_type = 'MATERIAL'
        scene.display.shading.show_shadows = True
        scene.display.shading.background_type = 'WORLD'
        scene.world = bpy.data.worlds.new('Reference background')
        scene.world.color = (.08, .08, .08)
        camera = bpy.data.cameras.new('Reference review')
        obj = bpy.data.objects.new('Reference review', camera)
        scene.collection.objects.link(obj)
        obj.location = (2.8, -6, 2.4)
        obj.rotation_euler = (Vector((0, 0, .95)) - obj.location).to_track_quat('-Z', 'Y').to_euler()
        camera.type, camera.ortho_scale = 'ORTHO', 2.25
        scene.camera = obj
        scene.render.resolution_x, scene.render.resolution_y = 1000, 1200
        scene.render.resolution_percentage = 100
        scene.render.filepath = str(output / 'preview.png')
        bpy.ops.render.render(write_still=True)


if __name__ == '__main__':
    main()
