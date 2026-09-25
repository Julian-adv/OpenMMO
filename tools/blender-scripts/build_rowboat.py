"""Build the rowboat and icon: blender -b -P tools/blender-scripts/build_rowboat.py.

Origin is the waterline; bow points -Y (glTF +Z).
"""
import math
import sys
from pathlib import Path

import bpy
from mathutils import Quaternion, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, principled, render_icon

ROOT = Path(__file__).resolve().parents[2]

LENGTH = 3.4
HALF_BEAM = 0.62
DEPTH = 0.55
DRAFT = 0.15
PLANK = 0.05
STATIONS = 37
RIB_POINTS = 19
# Hide the water plane inside the hull.
SOLE_Z = 0.06
WOOD_LENGTH = 2.0
WOOD_WIDTH = 0.8


def wood_material(name, image, roughness):
    material = principled(name, (1, 1, 1, 1), roughness)
    texture = material.node_tree.nodes.new('ShaderNodeTexImage')
    texture.image = image
    texture.extension = 'REPEAT'
    material.node_tree.links.new(
        texture.outputs['Color'],
        material.node_tree.nodes['Principled BSDF'].inputs['Base Color'],
    )
    return material


def map_box_wood(obj, grain_axis):
    mesh = obj.data
    uv = mesh.uv_layers.active or mesh.uv_layers.new(name='WoodUV')
    for face in mesh.polygons:
        axes = [axis for axis in range(3) if abs(face.normal[axis]) < 0.5]
        along = grain_axis if grain_axis in axes else axes[0]
        across = next(axis for axis in axes if axis != along)
        for loop in face.loop_indices:
            point = mesh.vertices[mesh.loops[loop].vertex_index].co
            uv.data[loop].uv = (
                point[along] * obj.scale[along] / WOOD_LENGTH + 0.5,
                point[across] * obj.scale[across] / WOOD_WIDTH + 0.3,
            )


def smoothstep(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def half_beam(t):
    """Narrow stem, widest amidships, flat transom."""
    if t < 0.5:
        return HALF_BEAM * (0.10 + 0.90 * smoothstep(t / 0.5) ** 0.6)
    return HALF_BEAM * (1.0 - 0.38 * smoothstep((t - 0.5) / 0.5) ** 1.4)


def keel_z(t):
    return -DRAFT + 0.22 * abs(2.0 * t - 1.0) ** 2.6


def rim_z(t):
    return DEPTH - DRAFT + 0.13 * (2.0 * t - 1.0) ** 2


def interior_half_width(t, z):
    """Inner half-width at `z`, or None outside the hull."""
    hb = max(half_beam(t) - PLANK, 0.004)
    bottom = keel_z(t) + PLANK
    top = rim_z(t)
    if z <= bottom or z >= top or top - bottom < 1e-6:
        return None
    ratio = (z - bottom) / (top - bottom)
    a = math.acos(min(max(1.0 - ratio ** (1.0 / 0.85), -1.0), 1.0))
    return hb * math.sin(a) ** 0.8


def rib(t, inset):
    """One cross section, port rim over the keel to starboard rim."""
    hb = max(half_beam(t) - inset, 0.004)
    bottom = keel_z(t) + (inset if inset else 0.0)
    top = rim_z(t)
    points = []
    for j in range(RIB_POINTS):
        a = (j / (RIB_POINTS - 1) - 0.5) * math.pi
        x = hb * math.copysign(abs(math.sin(a)) ** 0.8, a)
        z = bottom + (top - bottom) * (1.0 - math.cos(a)) ** 0.85
        points.append((x, LENGTH * (t - 0.5), z))
    return points


def build_hull(material):
    verts = []
    for inset in (0.0, PLANK):
        for i in range(STATIONS):
            t = i / (STATIONS - 1)
            if inset:
                if i == 0:
                    t += PLANK / LENGTH
                elif i == STATIONS - 1:
                    t -= PLANK / LENGTH
            points = rib(t, inset)
            if i == 0:
                points = [(0.0, y, z) for _, y, z in points]
            verts.extend(points)
    shell = STATIONS * RIB_POINTS

    def outer(i, j):
        return i * RIB_POINTS + j

    def inner(i, j):
        return shell + i * RIB_POINTS + j

    faces = []
    rim_faces = set()
    for i in range(STATIONS - 1):
        for j in range(RIB_POINTS - 1):
            faces.append((outer(i, j), outer(i, j + 1), outer(i + 1, j + 1), outer(i + 1, j)))
            faces.append((inner(i, j), inner(i, j + 1), inner(i + 1, j + 1), inner(i + 1, j)))
        for j in (0, RIB_POINTS - 1):
            rim_faces.add(len(faces))
            faces.append((outer(i, j), outer(i + 1, j), inner(i + 1, j), inner(i, j)))
    stern = STATIONS - 1
    transom_faces = len(faces)
    faces.append(tuple(outer(stern, j) for j in reversed(range(RIB_POINTS))))
    faces.append(tuple(inner(stern, j) for j in range(RIB_POINTS)))
    faces.append((
        outer(stern, 0), inner(stern, 0),
        inner(stern, RIB_POINTS - 1), outer(stern, RIB_POINTS - 1),
    ))

    mesh = bpy.data.meshes.new('Hull')
    mesh.from_pydata(verts, [], faces)
    mesh.materials.append(material)
    uv = mesh.uv_layers.new(name='WoodUV')
    for face in mesh.polygons:
        face.use_smooth = face.index < transom_faces
        for loop in face.loop_indices:
            vertex = mesh.loops[loop].vertex_index
            x, y, z = verts[vertex]
            if face.index == transom_faces + 2:
                coords = (x / WOOD_LENGTH + 0.5, y / WOOD_WIDTH + 0.3)
            elif face.index >= transom_faces:
                coords = (x / WOOD_LENGTH + 0.5, z / WOOD_WIDTH)
            elif face.index in rim_faces:
                coords = ((y + LENGTH / 2) / WOOD_LENGTH, x / WOOD_WIDTH + 0.3)
            else:
                j = vertex % RIB_POINTS
                coords = (
                    (y + LENGTH / 2) / WOOD_LENGTH,
                    abs(2 * j / (RIB_POINTS - 1) - 1),
                )
            uv.data[loop].uv = coords
    obj = bpy.data.objects.new('Hull', mesh)
    bpy.context.collection.objects.link(obj)

    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.mesh.remove_doubles(threshold=1e-6)
    bpy.ops.mesh.normals_make_consistent(inside=False)
    bpy.ops.object.mode_set(mode='OBJECT')
    obj.select_set(False)
    return obj


def build_sole(material):
    """Planking laid across the ribs, hiding the water the hull sits in."""
    verts = []
    spans = []
    for i in range(STATIONS):
        t = i / (STATIONS - 1)
        half = interior_half_width(t, SOLE_Z)
        if half is None:
            spans.append(None)
            continue
        y = LENGTH * (t - 0.5)
        spans.append(len(verts))
        verts.append((-half, y, SOLE_Z))
        verts.append((half, y, SOLE_Z))

    faces = []
    for a, b in zip(spans, spans[1:]):
        if a is None or b is None:
            continue
        faces.append((a, a + 1, b + 1, b))

    mesh = bpy.data.meshes.new('Sole')
    mesh.from_pydata(verts, [], faces)
    mesh.materials.append(material)
    uv = mesh.uv_layers.new(name='WoodUV')
    for loop in mesh.loops:
        x, y, _ = verts[loop.vertex_index]
        uv.data[loop.index].uv = ((y + LENGTH / 2) / WOOD_LENGTH, x / WOOD_WIDTH + 0.5)
    obj = bpy.data.objects.new('Sole', mesh)
    bpy.context.collection.objects.link(obj)
    return obj


def add_box(name, location, scale, material, grain_axis=0):
    bpy.ops.mesh.primitive_cube_add(size=2.0, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.data.name = name
    obj.scale = scale
    obj.data.materials.append(material)
    map_box_wood(obj, grain_axis)
    return obj


def add_thwart(name, t, material):
    y = LENGTH * (t - 0.5)
    return add_box(
        name,
        (0.0, y, keel_z(t) + 0.30),
        (half_beam(t) - PLANK * 0.5, 0.075, 0.022),
        material,
    )


def add_oar(name, side, material):
    """Build along +Z, then lay along -Y for client-side rowing."""
    oar_len = 2.2
    bpy.ops.mesh.primitive_cylinder_add(radius=0.026, depth=oar_len, location=(0, 0, 0))
    shaft = bpy.context.object
    shaft.data.materials.append(material)
    uv = shaft.data.uv_layers.active
    for face in shaft.data.polygons:
        for loop in face.loop_indices:
            x, y, z = shaft.data.vertices[shaft.data.loops[loop].vertex_index].co
            if abs(face.normal.z) > 0.5:
                coords = (x / WOOD_LENGTH + 0.5, y / WOOD_WIDTH + 0.3)
            else:
                coords = (z / WOOD_LENGTH + 0.5, math.atan2(y, x) / (2 * math.pi) * 0.15 + 0.3)
            uv.data[loop].uv = coords
    blade = add_box('OarBlade', (0.0, 0.0, oar_len * 0.42), (0.075, 0.012, 0.30), material, grain_axis=2)
    bpy.ops.object.select_all(action='DESELECT')
    shaft.select_set(True)
    blade.select_set(True)
    bpy.context.view_layer.objects.active = shaft
    bpy.ops.object.join()
    shaft.name = name
    shaft.data.name = name
    shaft.rotation_euler = (math.radians(90.0), 0.0, side * math.radians(6.0))
    shaft.location = (side * (HALF_BEAM - 0.14), -0.25, DEPTH - DRAFT - 0.14)
    return shaft


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    source = ROOT / 'assets/rowboat'
    texture = bpy.data.images.load(str(source / 'wood_albedo.png'))
    texture.scale(1024, 1024)
    texture.pack()
    oak = wood_material('BoatOak', texture, 0.82)
    trim = wood_material('BoatTrim', texture, 0.7)

    hull = build_hull(oak)
    parts = [hull]
    parts.append(build_sole(trim))
    parts.append(add_thwart('ThwartBow', 0.30, trim))
    parts.append(add_thwart('ThwartMid', 0.50, trim))
    parts.append(add_thwart('ThwartStern', 0.72, trim))
    oars = [add_oar('OarPort', -1.0, trim), add_oar('OarStarboard', 1.0, trim)]

    seat = bpy.data.objects.new('RideSeat', None)
    seat.empty_display_size = 0.15
    seat.location = (0.0, 0.0, keel_z(0.5) + 0.32)
    seat.rotation_euler.z = math.pi
    bpy.context.collection.objects.link(seat)

    exported = parts + oars
    bpy.ops.object.select_all(action='DESELECT')
    for obj in exported:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = hull
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    for oar, side in zip(oars, (-1, 1)):
        angle = side * math.radians(6.0)
        handle = Vector((-math.sin(angle), math.cos(angle), 0.0))
        rotation = Quaternion(
            (-math.cos(angle), -math.sin(angle), 0.0), math.radians(9.0),
        )
        oar.location += handle - rotation @ handle
        oar.rotation_mode = 'QUATERNION'
        oar.rotation_quaternion = rotation
    seat.select_set(True)

    source.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(source / 'rowboat.blend'))
    bpy.ops.export_scene.gltf(
        filepath=str(ROOT / 'client/public/models/mounts/rowboat.glb'),
        export_format='GLB',
        use_selection=True,
        export_animations=False,
        export_image_format='WEBP',
        export_image_quality=90,
    )

    world = bpy.data.worlds.new('World')
    world.use_nodes = True
    world.node_tree.nodes['Background'].inputs[0].default_value = (0.3, 0.3, 0.3, 1)
    bpy.context.scene.world = world
    add_light((0.8, -1.4, 2.0), 260, 2.0)
    add_light((-1.6, -0.8, 1.2), 110, 2.4)
    add_light((0.0, 1.8, 1.2), 90, 2.4)
    render_icon(
        exported,
        str(ROOT / 'client/public/items/objects/rowboat.png'),
        (math.radians(-32), 0.0, math.radians(-40)),
        1.06,
    )


main()
