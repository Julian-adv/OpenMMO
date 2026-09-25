"""Build the fishing rod, reel, guide line, icon, and inspection renders."""

import argparse
import json
import math
from pathlib import Path
import sys

import bpy
from mathutils import Matrix, Vector
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, principled, render_icon

ROOT = Path(__file__).resolve().parents[2]
GRIP_GLTF = Vector((-0.108, 0, 0))
OLD_TIP_GLTF = Vector((-0.051, 2.1173, -2.128))
LENGTH = (OLD_TIP_GLTF - GRIP_GLTF).length
REEL_CENTER = Vector((0.221, 0, -0.111))
PARTS = []


def mesh_object(name, vertices, faces, material, smooth=True):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(vertices, [], faces)
    mesh.materials.append(material)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    for face in mesh.polygons:
        face.use_smooth = smooth and len(face.vertices) == 4
    PARTS.append(obj)
    return obj


def lathe(name, profile, material, center=(0, 0, 0), axis=(1, 0, 0), sides=24):
    profile = [(float(along), float(radius)) for along, radius in profile]
    axis = Vector(axis).normalized()
    radial = axis.orthogonal().normalized()
    tangent = axis.cross(radial)
    center = Vector(center)
    vertices = [
        center + axis * along + radius * (
            radial * math.cos(math.tau * j / sides)
            + tangent * math.sin(math.tau * j / sides)
        )
        for along, radius in profile for j in range(sides)
    ]
    faces = []
    for i in range(len(profile) - 1):
        for j in range(sides):
            a, b = i * sides + j, i * sides + (j + 1) % sides
            faces.append((a, b, b + sides, a + sides))
    faces.extend((tuple(reversed(range(sides))), tuple(
        (len(profile) - 1) * sides + j for j in range(sides)
    )))
    obj = mesh_object(name, vertices, faces, material)
    uv = obj.data.uv_layers.new(name="SurfaceUV")
    for polygon in obj.data.polygons:
        indices = [obj.data.loops[i].vertex_index for i in polygon.loop_indices]
        seam = any(i % sides == 0 for i in indices) and any(i % sides == sides - 1 for i in indices)
        for loop_index, vertex_index in zip(polygon.loop_indices, indices):
            ring, side = divmod(vertex_index, sides)
            u = 1 if seam and side == 0 else side / sides
            uv.data[loop_index].uv = (u, profile[ring][0] * 2)
    return obj


def tube(name, points, radius, material, sides=6):
    points = [Vector(point) for point in points]
    vertices, faces = [], []
    for i, point in enumerate(points):
        direction = (points[min(i + 1, len(points) - 1)] - points[max(0, i - 1)]).normalized()
        radial = direction.orthogonal().normalized()
        tangent = direction.cross(radial)
        for j in range(sides):
            angle = j * math.tau / sides
            vertices.append(point + radius * (radial * math.cos(angle) + tangent * math.sin(angle)))
        if i:
            for j in range(sides):
                a, b = (i - 1) * sides + j, (i - 1) * sides + (j + 1) % sides
                faces.append((a, b, b + sides, a + sides))
    faces.extend((tuple(reversed(range(sides))), tuple(
        (len(points) - 1) * sides + j for j in range(sides)
    )))
    return mesh_object(name, vertices, faces, material)


def ring(name, center, radius, thickness, material, axis=(1, 0, 0), segments=20, sides=6):
    axis = Vector(axis).normalized()
    radial = axis.orthogonal().normalized()
    tangent = axis.cross(radial)
    vertices, faces = [], []
    for i in range(segments):
        direction = radial * math.cos(math.tau * i / segments) + tangent * math.sin(math.tau * i / segments)
        for j in range(sides):
            angle = math.tau * j / sides
            vertices.append(Vector(center) + direction * (radius + thickness * math.cos(angle)) + axis * thickness * math.sin(angle))
            a, b = i * sides + j, i * sides + (j + 1) % sides
            c, d = ((i + 1) % segments) * sides + j, ((i + 1) % segments) * sides + (j + 1) % sides
            faces.append((a, c, d, b))
    return mesh_object(name, vertices, faces, material)


def surface_material(name, base, roughness, kind):
    material = principled(name, (1, 1, 1, 1), roughness)
    y, x = np.mgrid[0:512, 0:512] / 512
    rng = np.random.default_rng(21)
    if kind == "wood":
        phase = x * 68 + 0.6 * np.sin(y * math.tau) + 0.25 * np.sin(y * 19 + x * 7)
        grain = np.maximum(0, np.sin(phase * math.tau)) ** 12
        shade = 0.92 - grain * 0.25 + 0.07 * np.sin(x * 31 + y * 2) + rng.normal(0, 0.022, x.shape)
    else:
        shade = 0.86 + rng.normal(0, 0.08, x.shape) + 0.05 * np.sin(x * 380) * np.sin(y * 410)
    pixels = np.ones((512, 512, 4), dtype=np.float32)
    pixels[:, :, :3] = np.clip(shade[:, :, None] * np.array(base), 0, 1)
    image = bpy.data.images.new(name + "Albedo", width=512, height=512)
    image.pixels.foreach_set(pixels.ravel())
    image.pack()
    texture = material.node_tree.nodes.new("ShaderNodeTexImage")
    texture.image = image
    material.node_tree.links.new(texture.outputs["Color"], material.node_tree.nodes["Principled BSDF"].inputs["Base Color"])
    return material


def make_rod():
    wood = surface_material("HoneyAsh", (0.53, 0.32, 0.14), 0.43, "wood")
    leather = surface_material("BrownLeather", (0.22, 0.105, 0.05), 0.78, "leather")
    brass = principled("AgedBrass", (0.40, 0.23, 0.075, 1), 0.34, 0.75)
    iron = principled("DarkIron", (0.07, 0.085, 0.09, 1), 0.43, 0.65)
    binding = principled("BurgundyBinding", (0.11, 0.023, 0.013, 1), 0.79)
    line = principled("LinenFishingLine", (0.58, 0.52, 0.37, 1), 0.87)

    def shaft_radius(s):
        return 0.019 - 0.0155 * (max(0, s - 0.16) / (LENGTH - 0.16)) ** 0.8

    lathe("AshBlank", [(s, shaft_radius(s)) for s in np.linspace(0.1, LENGTH, 20)], wood, sides=16)
    grip_profile = [(-0.125, 0.023), (-0.116, 0.028), (-0.08, 0.031),
                    (0.045, 0.030), (0.12, 0.024), (0.135, 0.020)]
    lathe("LeatherGrip", grip_profile, leather, sides=24)
    lathe("WoodenButt", [(-0.272, 0.021), (-0.265, 0.022), (-0.12, 0.022)], wood)
    for s, radius, width in [(-0.274, 0.024, 0.009), (REEL_CENTER.x - 0.049, 0.022, 0.012),
                              (REEL_CENTER.x + 0.058, 0.022, 0.012), (0.129, 0.023, 0.014),
                              (1.14, shaft_radius(1.14) + 0.001, 0.026),
                              (2.19, shaft_radius(2.19) + 0.001, 0.020)]:
        lathe("BrassFerrule", [(s - width / 2, radius * 0.94), (s - width * 0.35, radius),
              (s + width * 0.35, radius), (s + width / 2, radius * 0.94)], brass, sides=16)

    seam = []
    for i in range(181):
        t = i / 180
        x = -0.107 + t * 0.216
        radius = float(np.interp(x, *zip(*grip_profile))) + 0.0004
        seam.append((x, radius * math.cos(t * math.tau * 9), radius * math.sin(t * math.tau * 9)))
    tube("GripWrapSeam", seam, 0.00085, binding, sides=4)

    reel = REEL_CENTER
    tube("ReelFoot", [reel + Vector((-0.051, 0, 0.087)),
                      reel + Vector((0.056, 0, 0.087))], 0.008, iron, sides=8)
    tube("ReelStem", [reel + Vector((-0.001, 0, 0.089)),
                      reel + Vector((-0.021, 0, 0.060)),
                      reel + Vector((-0.021, 0, 0.031))], 0.009, brass, sides=8)
    rotor_start = len(PARTS)
    lathe("WoundLineSpool", [(-0.027, 0.052), (-0.022, 0.057), (0.022, 0.057), (0.027, 0.052)], line, reel, (0, 1, 0), sides=32)
    for i in range(9):
        ring("SpoolWinding", reel + Vector((0, -0.022 + i * 0.0055, 0)), 0.057, 0.0014, line, (0, 1, 0), segments=24, sides=4)
    lathe("ReelAxle", [(-0.042, 0.016), (-0.035, 0.019), (0.036, 0.019), (0.041, 0.014)], brass, reel, (0, 1, 0), sides=16)
    for side in (-1, 1):
        center = reel + Vector((0, side * 0.034, 0))
        ring("ReelRim", center, 0.076, 0.004, brass, (0, 1, 0), segments=32)
        ring("ReelInnerRim", center, 0.061, 0.0025, iron, (0, 1, 0), segments=24)
        for i in range(8):
            angle = i * math.tau / 8
            radial = Vector((math.cos(angle), 0, math.sin(angle)))
            tube("ReelSpoke", [center + radial * 0.018, center + radial * 0.074], 0.004, brass, sides=4)
    for i in range(4):
        angle = (i + 0.5) * math.tau / 4
        radial = Vector((math.cos(angle), 0, math.sin(angle)))
        lathe("ReelFramePin", [(-0.034, 0.003), (0.034, 0.003)], iron,
              reel + radial * 0.071, (0, 1, 0), sides=8)
    crank_end = reel + Vector((0.040, -0.05, -0.029))
    tube("ReelCrank", [reel + Vector((0, -0.045, 0)), crank_end], 0.0055, brass, sides=6)
    lathe("CrankHandle", [(0, 0.008), (0.006, 0.011), (0.030, 0.011), (0.036, 0.007)], wood,
          crank_end, (0, -1, 0), sides=12)
    rotating_parts = PARTS[rotor_start:]

    guides = [(0.36, 0.023), (0.86, 0.019), (1.42, 0.015), (1.95, 0.012),
              (2.39, 0.009), (2.73, 0.007), (LENGTH, 0.0055)]
    line_points = [reel + Vector((0.004, 0, 0.059))]
    for i, (s, radius) in enumerate(guides):
        blank_radius = shaft_radius(s)
        center = (s, 0, -(blank_radius + radius + 0.001))
        ring(f"LineGuide_{i + 1:02d}", center, radius, 0.0015 if i < 4 else 0.001, brass, segments=16)
        if i < len(guides) - 1:
            tube("GuideFoot", [(s - 0.032, 0, -blank_radius),
                 (s, 0, center[2] + radius), (s + 0.032, 0, -blank_radius)], 0.0015, iron, sides=4)
            for offset in (-0.026, 0.026):
                lathe("GuideBinding", [(s + offset - 0.008, blank_radius + 0.0007),
                      (s + offset + 0.008, blank_radius + 0.0007)], binding, sides=12)
        line_points.append(center)
    tube("ReelToTipLine", line_points, 0.00065, line, sides=6)
    return Vector(line_points[-1]), rotating_parts


def game_transform():
    along = (OLD_TIP_GLTF - GRIP_GLTF).normalized()
    up = -Vector((-0.9941617119, -0.0619949793, -0.0883125880))
    side = up.cross(along).normalized()
    up = along.cross(side).normalized()
    to_blender = Matrix(((1, 0, 0), (0, 0, -1), (0, 1, 0)))
    basis = to_blender @ Matrix((along, side, up)).transposed()
    transform = basis.to_4x4()
    transform.translation = to_blender @ GRIP_GLTF
    return transform


def render_preview(path, camera_location, target, scale, resolution):
    scene = bpy.context.scene
    camera = scene.camera
    camera.location = camera_location
    camera.rotation_euler = (Vector(target) - camera.location).to_track_quat("-Z", "Y").to_euler()
    camera.data.ortho_scale = scale
    scene.render.resolution_x, scene.render.resolution_y = resolution
    scene.render.film_transparent = False
    scene.view_settings.view_transform = "AgX"
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-root", type=Path, default=ROOT)
    args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
    source = args.output_root / "assets/fishing_rod"
    model = args.output_root / "client/public/models/weapons/fishing_rod.glb"
    icon = args.output_root / "client/public/items/weapons/fishing_rod.png"
    for path in (source, model.parent, icon.parent):
        path.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    tip_location, rotating_parts = make_rod()
    transform = game_transform()
    for obj in PARTS:
        obj.data.transform(transform)

    def join_parts(parts, name):
        bpy.ops.object.select_all(action="DESELECT")
        for obj in parts:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = parts[0]
        bpy.ops.object.join()
        obj = bpy.context.object
        obj.name = obj.data.name = name
        obj.data.calc_loop_triangles()
        return obj

    def anchor(name, position, parent):
        obj = bpy.data.objects.new(name, None)
        bpy.context.collection.objects.link(obj)
        obj.parent = parent
        obj.location = position
        return obj

    static_parts = [obj for obj in PARTS if obj not in rotating_parts]
    item = join_parts(static_parts, "fishing_rod")
    rotor_mesh = join_parts(rotating_parts, "reel_spool")
    rotor = anchor("reel_rotor", transform @ REEL_CENTER, item)
    rotor_mesh.data.transform(Matrix.Translation(-rotor.location))
    rotor_mesh.parent = rotor
    basis = transform.to_3x3()
    anchor("reel_axis", basis @ Vector((0, 0.05, 0)), rotor)
    anchor("reel_handle", basis @ Vector((0.040, -0.068, -0.029)), rotor)
    anchor("rod_grip", transform @ Vector((0, 0, 0)), item)
    tip = anchor("rod_tip", transform @ tip_location, item)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(filepath=str(model), export_format="GLB", use_selection=True,
                              export_animations=False, export_yup=True, export_image_format="WEBP",
                              export_image_quality=90, export_extras=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(source / "fishing_rod.blend"))
    metadata = {
        "created": "2026-09-21", "tool": "Blender " + bpy.app.version_string,
        "source": "tools/blender-scripts/build_fishing_rod.py",
        "license": "Repository LICENSE (PolyForm Noncommercial 1.0.0)",
        "method": "Original procedural geometry and seeded wood/leather textures; no external assets",
        "vertices": sum(len(obj.data.vertices) for obj in (item, rotor_mesh)),
        "polygons": sum(len(obj.data.polygons) for obj in (item, rotor_mesh)),
        "triangles": sum(len(obj.data.loop_triangles) for obj in (item, rotor_mesh)),
        "glb_bytes": model.stat().st_size,
        "grip_gltf": list(GRIP_GLTF), "rod_tip_gltf": [tip.location.x, tip.location.z, -tip.location.y],
        "reel_center_rod_xyz": list(REEL_CENTER),
        "reel_placement": "Ahead of the leather grip toward the tip; moved forward 0.42 m from the first version",
        "animation_anchors": ["reel_rotor", "reel_axis", "reel_handle", "rod_grip", "rod_tip"],
        "line": "Reel through seven guides to rod_tip; no hook, bobber, or free hanging line",
    }
    (source / "generation.json").write_text(json.dumps(metadata, indent=2) + "\n")

    bpy.context.view_layer.update()
    rotor_world = rotor_mesh.matrix_world.copy()
    rotor_mesh.parent = None
    rotor_mesh.matrix_world = rotor_world
    item = join_parts([item, rotor_mesh], "fishing_rod_preview")
    for obj in list(bpy.context.scene.objects):
        if obj.type == "EMPTY":
            bpy.data.objects.remove(obj, do_unlink=True)
    item.data.transform(transform.inverted())
    world = bpy.data.worlds.new("StudioWorld")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.07, 0.09, 0.11, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.8
    bpy.context.scene.world = world
    for location, energy, size in [((1, -2, 3), 260, 3), ((-1, 1, 1), 160, 2), ((2, 2, 2), 230, 2)]:
        add_light(location, energy, size, aim=(1.1, 0, 0))
    bpy.context.scene.view_settings.exposure = -1.4
    render_icon([item], str(icon), (math.radians(68), math.radians(-8), math.radians(44)), margin=1.10)
    bpy.data.images["Render Result"].save_render(str(source / "fishing_rod-render.png"))
    bpy.context.scene.view_settings.exposure = 0
    item.parent = None
    item.matrix_world = Matrix.Identity(4)
    render_preview(source / "fishing_rod-overview.png", (1.4, -4.5, 2.8), (1.35, 0, -0.015), 3.7, (1600, 600))
    render_preview(source / "fishing_rod-reel.png", (0.31, -0.8, 0.35), (0.06, 0, -0.055), 0.82, (1100, 850))
    print("FISHING_ROD_RESULT", json.dumps(metadata))


if __name__ == "__main__":
    main()
