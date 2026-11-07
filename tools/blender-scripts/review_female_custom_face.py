import argparse
import json
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('model', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--scale', type=float, default=1)
    parser.add_argument('--blend', type=Path)
    parser.add_argument('--hair', type=Path, action='append', default=[])
    parser.add_argument('--snapshots', type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:])
    args.output.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(args.model.resolve()))
    for obj in list(bpy.context.scene.objects):
        if not obj.parent:
            obj.scale *= args.scale
        if obj.type == 'ARMATURE':
            obj.data.pose_position = 'REST'
    rig = next((obj for obj in bpy.data.objects if obj.type == 'ARMATURE'), None)
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 24
    scene.cycles.use_denoising = True
    scene.world = bpy.data.worlds.new('Face review')
    scene.world.use_nodes = True
    background = scene.world.node_tree.nodes['Background']
    background.inputs[0].default_value = (.18, .18, .18, 1)
    background.inputs[1].default_value = .6
    for location, energy in [((-3, -4, 5), 450), ((4, -2, 3), 280), ((0, 3, 4), 350)]:
        bpy.ops.object.light_add(type='AREA', location=location)
        light = bpy.context.object
        light.data.energy, light.data.size = energy, 4
        light.rotation_euler = (Vector((0, 0, 1.65)) - light.location).to_track_quat('-Z', 'Y').to_euler()
    bpy.ops.object.camera_add()
    camera = bpy.context.object
    camera.data.type = 'ORTHO'
    scene.camera = camera
    scene.render.resolution_x, scene.render.resolution_y = 640, 800
    scene.render.resolution_percentage = 100
    scene.view_settings.view_transform = 'AgX'
    target = Vector((0, 0, 1.61))
    def render(name, offset, target=target, scale=.5):
        camera.location = target + Vector(offset)
        camera.rotation_euler = (target - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str((args.output / f'{name}.png').resolve())
        bpy.ops.render.render(write_still=True)
    views = [('front', (0, -4, 0)), ('angle', (2.5, -4, 0)), ('side', (4, 0, 0)), ('back', (0, 4, 0))]
    for name, offset in views:
        render(name, offset)
    scene.render.resolution_x, scene.render.resolution_y = 1200, 450
    render('eyes', (0, -4, 0), Vector((0, -.065, 1.682)), .16)
    scene.render.resolution_x, scene.render.resolution_y = 640, 800
    render('eyes-side', (4, 0, 0), Vector((0, -.065, 1.680)), .08)
    render('eyes-other-side', (-4, 0, 0), Vector((0, -.065, 1.680)), .08)
    render('eyes-rear-side', (4, 3.14, 0), Vector((.03, -.065, 1.680)), .08)
    render('eyes-rear-other-side', (-4, 3.14, 0), Vector((-.0316, -.065, 1.680)), .08)
    render('eyes-top-side', (2.6, -2.4, 4.3), Vector((.03, -.06, 1.681)), .12)
    render('eyes-top-other-side', (-2.6, -2.4, 4.3), Vector((-.0316, -.06, 1.681)), .12)
    render('full-body', (0, -4, 0), Vector((0, 0, .93)), 2)
    hairs = []
    for path in args.hair:
        for obj in hairs:
            obj.hide_render = True
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=str(path.resolve()))
        imported = set(bpy.data.objects) - before
        meshes = [obj for obj in imported if obj.type == 'MESH']
        for obj in meshes:
            world = obj.matrix_world.copy()
            obj.parent = rig
            obj.matrix_world = world
            for modifier in obj.modifiers:
                if modifier.type == 'ARMATURE':
                    modifier.object = rig
        for obj in imported:
            if obj.type == 'ARMATURE':
                bpy.data.objects.remove(obj, do_unlink=True)
        hairs.extend(meshes)
        for name, offset in views:
            render(f'{path.stem}-{name}', offset)
    for obj in hairs:
        obj.hide_render = True
    if args.snapshots:
        rig.data.pose_position = 'POSE'
        rig.animation_data_clear()
        conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
        ordered = sorted(rig.pose.bones, key=lambda bone: len(bone.parent_recursive))
        for index, snapshot in enumerate(json.loads(args.snapshots.read_text()), 1):
            scene.frame_set(index * 10)
            for bone in ordered:
                values = snapshot['matrices'][bone.name]
                deformation = Matrix([values[i:i + 4] for i in range(0, 16, 4)]).transposed()
                bone.matrix = rig.matrix_world.inverted() @ conversion @ deformation @ conversion.inverted() @ rig.matrix_world @ bone.bone.matrix_local
                bpy.context.view_layer.update()
                bone.rotation_mode = 'QUATERNION'
                for channel in ['location', 'rotation_quaternion', 'scale']:
                    bone.keyframe_insert(data_path=channel, frame=index * 10)
            scene.timeline_markers.new(snapshot['clip'], frame=index * 10)
            point = rig.matrix_world @ rig.pose.bones['Head'].matrix.translation + Vector((0, 0, .03))
            render('motion-' + snapshot['clip'], (1.5, -4, .1), point, .65)
        scene.frame_set(0)
        rig.data.pose_position = 'REST'
    if args.blend:
        bpy.context.preferences.filepaths.save_version = 0
        bpy.ops.file.pack_all()
        bpy.ops.wm.save_as_mainfile(filepath=str(args.blend.resolve()))


if __name__ == '__main__':
    main()
