"""Shared steps for cutting a lid off a one-shell Tripo chest and rigging it to open.

Import it from a `-b -P` script:

    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import chest_rig
"""
import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb  # noqa: E402

FPS = 30


def import_shell(path):
    """The source's one mesh, transforms applied, everything else removed."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(path))
    body = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
    bpy.context.view_layer.objects.active = body
    body.select_set(True)
    bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    for obj in [o for o in bpy.context.scene.objects if o is not body]:
        bpy.data.objects.remove(obj)
    return body


def cut_lid(body, seam_z, in_lid, name):
    """Bisect at the seam and split the faces `in_lid(centre)` picks into a lid."""
    bm = bmesh.new()
    bm.from_mesh(body.data)
    geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
    bmesh.ops.bisect_plane(bm, geom=geom, plane_co=(0, 0, seam_z), plane_no=(0, 0, 1))
    for face in bm.faces:
        face.select = in_lid(face.calc_center_median())
    bm.to_mesh(body.data)
    bm.free()
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.separate(type='SELECTED')
    bpy.ops.object.mode_set(mode='OBJECT')
    lid = next(o for o in bpy.context.scene.objects if o.type == 'MESH' and o is not body)
    body.name = body.data.name = f'{name}_body'
    lid.name = lid.data.name = f'{name}_lid'
    return lid


def stone_patch(obj, above=-math.inf):
    """UV box of the largest upward face above a height: plain stone to tile the caps."""
    uv = obj.data.uv_layers.active.data
    face = max((p for p in obj.data.polygons if p.normal.z > 0.9 and p.center.z > above), key=lambda p: p.area)
    us = [uv[i].uv for i in face.loop_indices]
    return (min(u.x for u in us), min(u.y for u in us), max(u.x for u in us), max(u.y for u in us))


def mesh(name, verts, faces, material, uv_of):
    """A new object from polygons, each corner's UV given by `uv_of(vertex)`."""
    data = bpy.data.meshes.new(name)
    data.from_pydata(verts, [], faces)
    layer = data.uv_layers.new()
    for loop in data.loops:
        layer.data[loop.index].uv = uv_of(verts[loop.vertex_index])
    data.materials.append(material)
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def tile(patch, x0, x1, y0, y1):
    """Map a rectangle in x, y onto a UV patch."""
    u0, v0, u1, v1 = patch
    return lambda p: (u0 + (u1 - u0) * (p[0] - x0) / (x1 - x0), v0 + (v1 - v0) * (p[1] - y0) / (y1 - y0))


def cap(name, z, x0, x1, y0, y1, material, patch, down=False):
    face = (3, 2, 1, 0) if down else (0, 1, 2, 3)
    return mesh(name, [(x0, y0, z), (x1, y0, z), (x1, y1, z), (x0, y1, z)], [face], material,
                tile(patch, x0, x1, y0, y1))


def join(target, extra):
    bpy.ops.object.select_all(action='DESELECT')
    extra.select_set(True)
    target.select_set(True)
    bpy.context.view_layer.objects.active = target
    bpy.ops.object.join()


def seat_and_hinge(body, lid, height, hinge_y, hinge_z):
    """Scale to `height`, centre on the floor and pivot the lid on its hinge line.

    Returns the source bounds and the scale.
    """
    everything = [body, lid]
    low = Vector(min(v.co[i] for o in everything for v in o.data.vertices) for i in range(3))
    high = Vector(max(v.co[i] for o in everything for v in o.data.vertices) for i in range(3))
    scale = height / (high.z - low.z)
    centre = (low + high) / 2
    hinge = Vector((0, (hinge_y - centre.y) * scale, (hinge_z - low.z) * scale))
    for obj in everything:
        for v in obj.data.vertices:
            v.co = Vector(((v.co.x - centre.x) * scale, (v.co.y - centre.y) * scale, (v.co.z - low.z) * scale))
    for v in lid.data.vertices:
        v.co -= hinge
    lid.location = hinge
    lid.parent = body
    return low, high, scale


def rig_and_export(body, lid, open_deg, open_seconds, texture, work, out):
    """Shrink the textures, key `ChestOpen` (the plain chest's clip name), save and export."""
    for image in bpy.data.images:
        if image.size[0] > texture:
            image.scale(texture, texture)
    scene = bpy.context.scene
    scene.render.fps = FPS
    lid.rotation_mode = 'XYZ'
    lid.animation_data_create()
    lid.animation_data.action = bpy.data.actions.new('ChestOpen')
    end = round(open_seconds * FPS)
    lid.rotation_euler = (0, 0, 0)
    lid.keyframe_insert('rotation_euler', frame=1)
    lid.rotation_euler = (-math.radians(open_deg), 0, 0)
    lid.keyframe_insert('rotation_euler', frame=1 + end)
    scene.frame_start, scene.frame_end = 1, 1 + end
    work.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(work))
    bpy.ops.object.select_all(action='DESELECT')
    body.select_set(True)
    lid.select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=str(out), export_format='GLB', use_selection=True, export_animations=True,
        export_image_format='WEBP', export_image_quality=90, export_yup=True,
    )


def shade(path, material_name, shade, matte=False):
    """Darken a material in the exported GLB; `matte` also drops its shine."""
    gltf, binary = read_glb(path)
    for material in gltf['materials']:
        if material.get('name') == material_name:
            pbr = material.setdefault('pbrMetallicRoughness', {})
            pbr['baseColorFactor'] = [shade, shade, shade, 1.0]
            if matte:
                pbr.pop('metallicRoughnessTexture', None)
                pbr.update(metallicFactor=0.0, roughnessFactor=1.0)
    write_glb(path, gltf, binary)


def triangles(*objs):
    return sum(len(p.vertices) - 2 for o in objs for p in o.data.polygons)
