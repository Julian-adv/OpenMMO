"""Create the leather reins item and icon in Blender."""
import math
import sys
from pathlib import Path

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, principled, render_icon

ROOT = Path(__file__).resolve().parents[2]
bpy.ops.wm.read_factory_settings(use_empty=True)
leather = principled('ReinsLeather', (0.11, 0.035, 0.012, 1), 0.72)
brass = principled('ReinsBrass', (0.55, 0.30, 0.06, 1), 0.32, 0.8)
objects = []
for offset, angle in [(-0.035, -0.25), (0.035, 0.25)]:
    vertices=[]
    faces=[]
    for i in range(80):
        t=i/80*math.tau
        for radius,z in [(1.0,0.009),(0.91,0.009),(1.0,0.001),(0.91,0.001)]:
            x=0.075*math.cos(t)*radius
            y=0.155*math.sin(t)*radius
            vertices.append((offset+x*math.cos(angle)-y*math.sin(angle),x*math.sin(angle)+y*math.cos(angle),z))
    for i in range(80):
        a=i*4;b=((i+1)%80)*4
        faces.extend([(a,b,b+1,a+1),(a+2,a+3,b+3,b+2),(a,a+2,b+2,b),(a+1,b+1,b+3,a+3)])
    mesh=bpy.data.meshes.new('LeatherStrap');mesh.from_pydata(vertices,[],faces);mesh.materials.append(leather)
    obj=bpy.data.objects.new('LeatherStrap',mesh);bpy.context.collection.objects.link(obj);objects.append(obj)
for x in [-0.045,0.045]:
    bpy.ops.mesh.primitive_torus_add(major_radius=0.022,minor_radius=0.004,major_segments=32,minor_segments=8,location=(x,0.13,0.015))
    obj=bpy.context.object;obj.data.materials.append(brass);objects.append(obj)
for obj in objects:
    obj.select_set(True)
bpy.context.view_layer.objects.active=objects[0]
bpy.ops.object.transform_apply(location=True,rotation=True,scale=True)
source=ROOT/'assets/horse_reins';source.mkdir(parents=True,exist_ok=True)
bpy.ops.wm.save_as_mainfile(filepath=str(source/'horse_reins.blend'))
bpy.ops.export_scene.gltf(filepath=str(ROOT/'client/public/models/objects/horse_reins.glb'),export_format='GLB',use_selection=True,export_animations=False)
world=bpy.data.worlds.new('World');world.use_nodes=True;world.node_tree.nodes['Background'].inputs[0].default_value=(0.3,0.3,0.3,1);bpy.context.scene.world=world
add_light((0,0,2),100,1)
add_light((-1,1,1),70,1)
icon=ROOT/'client/public/items/objects/horse_reins.png'
render_icon(objects,str(icon),(math.radians(20),math.radians(-10),math.radians(-25)),1.15)
