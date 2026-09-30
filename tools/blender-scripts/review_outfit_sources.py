"""Render raw outfit sources and record geometry diagnostics without fitting them."""
import argparse
import hashlib
import json
import sys
from pathlib import Path

import bpy
import bmesh
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]


def diagnostics(objects):
    triangles = 0
    bounds = []
    meshes = []
    for obj in objects:
        obj.data.calc_loop_triangles()
        triangles += len(obj.data.loop_triangles)
        bounds.extend(obj.matrix_world @ Vector(v) for v in obj.bound_box)
        uv_layer = obj.data.uv_layers.active
        collapsed_uv = None
        if uv_layer:
            collapsed_uv = 0
            for tri in obj.data.loop_triangles:
                a, b, c = [uv_layer.data[i].uv for i in tri.loops]
                ab, ac = b - a, c - a
                collapsed_uv += abs(ab.x * ac.y - ab.y * ac.x) < 1e-10
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bmesh.ops.remove_doubles(bm, verts=list(bm.verts), dist=.00001)
        groups, unseen = [], set(bm.verts)
        while unseen:
            group, pending = set(), [next(iter(unseen))]
            while pending:
                v = pending.pop()
                if v in group:
                    continue
                group.add(v)
                pending.extend(e.other_vert(v) for e in v.link_edges if e.other_vert(v) not in group)
            unseen -= group
            groups.append(len(group))
        meshes.append(dict(name=obj.name, components_vertex_counts=sorted(groups, reverse=True),
                           boundary_edges=sum(e.is_boundary for e in bm.edges),
                           nonmanifold_edges=sum(not e.is_manifold for e in bm.edges),
                           degenerate_faces=sum(f.calc_area() < 1e-12 for f in bm.faces),
                           collapsed_uv_triangles=collapsed_uv))
        bm.free()
    low = Vector(tuple(min(v[i] for v in bounds) for i in range(3)))
    high = Vector(tuple(max(v[i] for v in bounds) for i in range(3)))
    return dict(triangles=triangles, mesh_count=len(objects), meshes=meshes,
                source_extent_blender=list(high - low),
                rigged=any(any(m.type == 'ARMATURE' for m in o.modifiers) for o in objects)), low, high


def text_label(value, position):
    curve = bpy.data.curves.new(value, 'FONT')
    curve.body = value
    curve.align_x = 'CENTER'
    curve.size = .11
    obj = bpy.data.objects.new(value, curve)
    bpy.context.collection.objects.link(obj)
    obj.location = position
    obj.rotation_euler = (1.5707963, 0, 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', required=True)
    parser.add_argument('--image', required=True)
    parser.add_argument('--report', required=True)
    parser.add_argument('--blend')
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:])
    config = json.loads((ROOT / args.config).read_text())
    bpy.ops.wm.read_factory_settings(use_empty=True)
    report = dict(status='raw sources, normalized separately for display; not assembled or fitted', parts=[])
    count = len(config['parts'])
    for index, part in enumerate(config['parts']):
        path = (ROOT / part['source_glb'] if 'source_glb' in part else
                ROOT / config['output_dir'] / part['id'] / 'source.glb')
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(path))
        objects = [o for o in set(bpy.data.objects) - before if o.type == 'MESH']
        diagnostic, low, high = diagnostics(objects)
        report['parts'].append(dict(id=part['id'], source=str(path.relative_to(ROOT)),
                                    sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                                    target_triangles=part['target_triangles'], **diagnostic))
        scale = min(1.65 / (high.z - low.z), 1.65 / (high.x - low.x))
        center = (low + high) / 2
        root = bpy.data.objects.new(part['id'], None)
        bpy.context.collection.objects.link(root)
        for obj in objects:
            world = obj.matrix_world.copy()
            obj.parent = root
            obj.matrix_world = world
            for polygon in obj.data.polygons:
                polygon.use_smooth = True
        root.scale = (scale,) * 3
        horizontal = (index - (count - 1) / 2) * 1.95
        root.location = Vector((horizontal, 0, 1.0)) - center * scale
        back = bpy.data.objects.new(part['id'] + '_back', None)
        bpy.context.collection.objects.link(back)
        for obj in objects:
            copy = obj.copy()
            bpy.context.collection.objects.link(copy)
            copy.parent = back
        back.scale = (scale,) * 3
        back.rotation_euler.z = 3.14159265
        back.location = Vector((horizontal, 0, -1.0)) + Vector((center.x, center.y, -center.z)) * scale
        text_label(part['id'] + f" / {diagnostic['triangles']} tri", (horizontal, -.8, 2.03))
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 32
    scene.world = bpy.data.worlds.new('Review background')
    scene.world.color = (.3, .3, .3)
    for location, energy, size in [((-3, -5, 6), 1600, 7), ((4, -3, 2), 1100, 6), ((0, 3, 5), 1200, 5)]:
        data = bpy.data.lights.new('Studio', 'AREA')
        data.energy, data.shape, data.size = energy, 'DISK', size
        obj = bpy.data.objects.new('Studio', data)
        scene.collection.objects.link(obj)
        obj.location = location
        obj.rotation_euler = (-obj.location).to_track_quat('-Z', 'Y').to_euler()
    camera = bpy.data.cameras.new('Review')
    obj = bpy.data.objects.new('Review', camera)
    scene.collection.objects.link(obj)
    obj.location = (0, -14, .4)
    obj.rotation_euler = (Vector((0, 0, .1)) - obj.location).to_track_quat('-Z', 'Y').to_euler()
    camera.type, camera.ortho_scale = 'ORTHO', max(10.2, count * 1.95 + .45)
    scene.camera = obj
    scene.render.resolution_x, scene.render.resolution_y = 1800, 850
    scene.render.resolution_percentage = 100
    scene.view_settings.view_transform = 'AgX'
    image, output = ROOT / args.image, ROOT / args.report
    image.parent.mkdir(parents=True, exist_ok=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + '\n')
    scene.render.filepath = str(image)
    if args.blend:
        blend = ROOT / args.blend
        blend.parent.mkdir(parents=True, exist_ok=True)
        bpy.ops.file.pack_all()
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.wm.save_as_mainfile(filepath=str(blend))
    bpy.ops.render.render(write_still=True)


if __name__ == '__main__':
    main()
