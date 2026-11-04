"""Render fitted female outfit parts on the female base, at rest and in sampled animation frames.

blender -b --python tools/blender-scripts/review_female_outfit.py -- --outfit ranger --parts gloves top --out DIR [--views hands] [--clip walk:8]
"""
import json
import sys
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
argv = sys.argv[sys.argv.index('--') + 1:]


def values(flag):
    if flag not in argv:
        return []
    i = argv.index(flag) + 1
    out = []
    while i < len(argv) and not argv[i].startswith('--'):
        out.append(argv[i])
        i += 1
    return out


parts = values('--parts')
DIRECTORY = ROOT / f"assets/items/female_{values('--outfit')[0]}_outfit"
out = Path(values('--out')[0])
views = values('--views') or ['full']
clips = values('--clip')
out.mkdir(parents=True, exist_ok=True)

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
body = bpy.data.objects['female_body']
hidden = set()
for part in parts:
    fitted = json.loads((next((DIRECTORY / part).glob('fitted_v*/fitting.json'))).read_text())
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(ROOT / fitted['output']['path']), bone_heuristic='TEMPERANCE')
    hidden |= set(json.loads((ROOT / fitted['output']['path']).with_name('body-occlusion.json').read_text())['hide_triangle_indices'])
    for obj in set(bpy.data.objects) - before:
        if obj.type == 'MESH':
            obj.modifiers.clear()
            obj.parent = rig
            obj.matrix_parent_inverse.identity()
            obj.modifiers.new('rig', 'ARMATURE').object = rig
        elif obj.type == 'ARMATURE':
            bpy.data.objects.remove(obj)
if '--no-hide' not in argv:
    import bmesh
    # glTF primitive 0 comes first in the imported body, so mask indices address the same faces.
    bm = bmesh.new()
    bm.from_mesh(body.data)
    bm.faces.ensure_lookup_table()
    bmesh.ops.delete(bm, geom=[bm.faces[i] for i in hidden], context='FACES_ONLY')
    bm.to_mesh(body.data)
    bm.free()
skin = bpy.data.materials.new('skin')
skin.use_nodes = True
skin.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value = (.8, .45, .35, 1)
body.data.materials.clear()
body.data.materials.append(skin)
body.hide_render = '--no-body' in argv

scene = bpy.context.scene
scene.render.engine = 'CYCLES'
scene.cycles.samples = 24
scene.cycles.use_denoising = False
scene.render.film_transparent = True
world = bpy.data.worlds.new('world')
world.use_nodes = True
world.node_tree.nodes['Background'].inputs['Color'].default_value = (1, 1, 1, 1)
world.node_tree.nodes['Background'].inputs['Strength'].default_value = .8
scene.world = world
sun = bpy.data.objects.new('sun', bpy.data.lights.new('sun', 'SUN'))
sun.data.energy = 3
sun.rotation_euler = (.6, .3, .4)
scene.collection.objects.link(sun)
scene.render.resolution_x = scene.render.resolution_y = 700
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
scene.collection.objects.link(cam)
cam.data.type = 'ORTHO'
scene.camera = cam


def bone_point(name):
    return rig.matrix_world @ rig.pose.bones[name].head


FRAMES = {
    'full': (lambda: Vector((0, 0, .95)), 2.0, [('front', (0, -1, 0)), ('back', (0, 1, 0)), ('side', (1, 0, 0))]),
    'torso': (lambda: Vector((0, 0, 1.3)), .9, [('front', (0, -1, 0)), ('back', (0, 1, 0)), ('side', (1, 0, 0)), ('angle', (.7, -1, .1))]),
    'hips': (lambda: Vector((0, 0, 1.0)), .7, [('front', (0, -1, 0)), ('back', (0, 1, 0)), ('side', (1, 0, 0))]),
    'head': (lambda: Vector((0, 0, 1.7)), .45, [('front', (0, -1, 0)), ('back', (0, 1, 0)), ('side', (1, 0, 0))]),
    'feet': (lambda: Vector((0, 0, .45)), 1.0, [('front', (0, -1, 0)), ('back', (0, 1, 0)), ('side', (1, 0, 0))]),
    'hands': (lambda: bone_point('LeftHand'), .35, [('top', (0, 0, 1)), ('front', (0, -1, 0)), ('back', (0, 1, 0)), ('palm', (0, 0, -1))]),
}


def render(tag):
    bpy.context.view_layer.update()
    for view in views:
        center, size, directions = FRAMES[view]
        c = center()
        cam.data.ortho_scale = size
        for name, d in directions:
            d = Vector(d).normalized()
            cam.location = c + d * 4
            cam.rotation_euler = (-d).to_track_quat('-Z', 'Y').to_euler()
            scene.render.filepath = str(out / f'{tag}-{view}-{name}.png')
            bpy.ops.render.render(write_still=True)


rig.animation_data_clear()
for b in rig.pose.bones:
    b.matrix_basis.identity()
render('rest')
for clip in clips:
    name, frame = clip.split(':')
    rig.animation_data_create()
    rig.animation_data.action = bpy.data.actions[name]
    scene.frame_set(int(frame))
    render(f'{name}{frame}')
