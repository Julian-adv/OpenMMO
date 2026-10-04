import importlib.util
import json
import tempfile
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
PARTS = ROOT / 'assets/modular_human_male_01/parts'
OUTPUT = PARTS / 'hair_tripo_wavy_v1'
IMAGES = ROOT / 'doc/images/characters/modular_human_male_01/customization'
spec = importlib.util.spec_from_file_location('review', ROOT / 'tools/blender-scripts/review_rogue_fitting.py')
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)


def main(directory):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(PARTS / 'fitted/base.glb'))
    rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
    for obj in bpy.data.objects:
        if obj.type == 'MESH':
            obj.hide_render = obj.get('region') == 'boot_ankles'
    review.import_part(OUTPUT / 'hair_wavy_bone.glb', rig)
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 24
    scene.world = bpy.data.worlds.new('Hair fitting studio')
    scene.world.color = (.14, .14, .14)
    for location, energy, size in [((-3, -4, 5), 500, 4), ((4, -2, 3), 350, 4), ((0, 3, 4), 450, 3)]:
        bpy.ops.object.light_add(type='AREA', location=location)
        light = bpy.context.object
        light.data.energy, light.data.size = energy, size
        light.rotation_euler = (Vector((0, 0, 1.7)) - light.location).to_track_quat('-Z', 'Y').to_euler()
    bpy.ops.object.camera_add()
    camera = bpy.context.object
    camera.data.type = 'ORTHO'
    scene.camera = camera
    scene.render.resolution_x, scene.render.resolution_y = 700, 900
    scene.render.resolution_percentage = 100
    scene.view_settings.view_transform = 'AgX'

    def render(name, location, target, scale):
        camera.location = location
        camera.rotation_euler = (Vector(target) - camera.location).to_track_quat('-Z', 'Y').to_euler()
        camera.data.ortho_scale = scale
        scene.render.filepath = str(directory / f'{name}.png')
        bpy.ops.render.render(write_still=True)

    render('back', (0, 5, 1.75), (0, 0, 1.73), .59)
    for name, location in [('front-close', (0, -5, 1.86)), ('front-side', (5, 0, 1.86))]:
        render(name, location, (0, 0, 1.83), .34)
    review.contact_sheet(directory, ['front-close', 'front-side'], ['FRONT', 'SIDE'], 2, IMAGES / 'hair-wavy-bone-fitted-v1-front-fit.png')
    for name, location in [('rear-left', (-2.5, 5, 1.70)), ('rear-right', (2.5, 5, 1.70))]:
        render(name, location, (0, 0, 1.69), .50)
    review.contact_sheet(directory, ['back', 'rear-left', 'rear-right'], ['BACK', 'REAR LEFT', 'REAR RIGHT'], 3, IMAGES / 'hair-wavy-bone-fitted-v1-rear-fill.png')
    snapshots = OUTPUT / 'animation-snapshots.json'
    if snapshots.exists():
        conversion = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
        ordered = sorted(rig.pose.bones, key=lambda b: len(b.parent_recursive))
        frames = json.loads(snapshots.read_text())
        for index, snapshot in enumerate(frames, 1):
            scene.frame_set(index * 10)
            for bone in ordered:
                values = snapshot['bone_deformation_matrices'][bone.name]
                deformation = Matrix([values[i:i + 4] for i in range(0, 16, 4)]).transposed()
                bone.matrix = rig.matrix_world.inverted() @ conversion @ deformation @ conversion.inverted() @ rig.matrix_world @ bone.bone.matrix_local
                bpy.context.view_layer.update()
                bone.rotation_mode = 'QUATERNION'
                for channel in ['location', 'rotation_quaternion', 'scale']:
                    bone.keyframe_insert(data_path=channel, frame=index * 10)
            scene.timeline_markers.new(snapshot['clip'], frame=index * 10)
            target = rig.matrix_world @ rig.pose.bones['Head'].matrix.translation + Vector((0, 0, .06))
            render(snapshot['clip'], target + Vector((-2.0, -5, .1)), target, .68)
        review.contact_sheet(directory, [s['clip'] for s in frames], [s['clip'].upper() for s in frames], 4, IMAGES / 'hair-wavy-bone-fitted-v1-motion.png')
        scene.frame_set(10)
    references = bpy.data.collections.new('Original Tripo hair - hidden')
    scene.collection.children.link(references)
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(OUTPUT / 'source.glb'))
    for obj in set(bpy.data.objects) - before:
        for collection in list(obj.users_collection):
            collection.objects.unlink(obj)
        references.objects.link(obj)
    references.hide_render = True
    references.hide_viewport = True
    bpy.ops.file.pack_all()
    bpy.context.preferences.filepaths.save_version = 0
    bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / 'wavy-hair-fitting.blend'))


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='wavy-hair-review-') as temporary:
        main(Path(temporary))
