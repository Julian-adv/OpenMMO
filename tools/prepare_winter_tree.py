"""Build client/public/models/objects/winter_tree.glb from the Sketchfab source.

Run: blender -b --factory-startup -P tools/prepare_winter_tree.py -- <source.glb> <out.glb> [lights.json]

Keeps the LOD1 crown of "Christmas tree low poly 4 lods" by David Montaña
(CC BY 4.0) and stands it at the origin at game scale. The source is a potted
tree on a bare trunk and reads as bare from the game camera, so the pot is
dropped and the trunk sunk until the boughs sweep the ground, the crown is
filled (three more turns of the boughs over a dark core), its few ornaments are
enlarged, and baubles, a skirt and a pile of gifts are added. Lists where the
bulbs and the star sit. See doc/assets/props.md.
"""

import json
import math
import random
import struct
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

# Height before the star is enlarged.
HEIGHT_M = 5.0
KEEP = ("Tree_lod1", "garland_lod1", "lights_lod1", "ornaments_lod1")
# The source's pot marks where the trunk stands.
POT = "base_lod1"
# Share of the source's height sunk below the ground: the pot and the bare trunk.
SUNK = 0.11
SMALL_TEXTURE = 512
CUTOUT = ("Prueba_01_LP_default", "04_-_Default")
# Sprig cards are thin: a lower cutoff keeps more of each needle.
FOLIAGE_CUTOFF = 0.35

CROWN_WIDTH = 1.03
# Extra turns of the boughs: (degrees about the trunk, width, height, lift).
BOUGH_TURNS = ((47, 0.84, 0.97, 0.0), (101, 1.0, 0.95, 0.12), (163, 0.64, 0.9, 0.22))
CORE_COLOR = "#27552b"
# Share of the crown's radius the dark core fills.
CORE_FILL = 0.5
CORE_HEIGHTS = (0.0, 0.5, 1.0, 1.6, 2.2, 2.8, 3.4)

ORNAMENT_SCALE = 1.35
STAR_SCALE = 1.1
# Pieces of one ornament sit closer together than this.
ORNAMENT_REACH = 0.34

# Baubles hang in rings: (height, count).
BAUBLE_RINGS = ((0.95, 9), (1.55, 8), (2.2, 7), (2.85, 5), (3.45, 4), (4.0, 3))
BAUBLE_RADII = (0.15, 0.125, 0.105)
BAUBLE_COLORS = ("#c2242b", "#e8b647", "#2f6cc0", "#c2242b", "#e8b647", "#f1ead9")
# How far out along the boughs a bauble hangs.
BAUBLE_REACH = 0.88

GOLD, RED, IVORY, GREEN, BLUE = "#edc35a", "#c2242b", "#f3eee2", "#1f6b3a", "#2b5fa8"
# Gifts: (width, depth, height, degrees around the trunk, distance, turn, lift, box, ribbon).
# The game camera looks at the tree from 225 degrees, so the pile is fullest there.
GIFTS = (
    (0.68, 0.68, 0.5, 236, 1.1, 15, 0.0, GREEN, GOLD),
    (0.32, 0.32, 0.24, 236, 1.1, 50, 0.5, RED, IVORY),
    (0.4, 0.4, 0.56, 188, 1.15, 40, 0.0, IVORY, RED),
    (0.32, 0.32, 0.28, 208, 1.4, 30, 0.0, BLUE, GOLD),
    (0.48, 0.7, 0.32, 277, 1.18, -20, 0.0, GOLD, RED),
    (0.6, 0.44, 0.32, 155, 1.15, 10, 0.0, BLUE, IVORY),
    (0.38, 0.38, 0.38, 312, 1.12, -30, 0.0, IVORY, GREEN),
    (0.4, 0.4, 0.42, 120, 1.1, 25, 0.0, RED, IVORY),
    (0.46, 0.34, 0.28, 348, 1.15, 35, 0.0, GREEN, RED),
    (0.42, 0.42, 0.4, 50, 1.1, 10, 0.0, RED, GOLD),
)

SKIRT_COLOR = "#5e1119"
# The skirt's profile from the trunk out, then its fur trim: (radius, height).
SKIRT_PROFILE = ((0.3, 0.16), (0.95, 0.09), (1.5, 0.04))
TRIM_PROFILE = ((1.5, 0.04), (1.55, 0.09), (1.63, 0.085), (1.69, 0.0))
SKIRT_FOLDS = 9
SKIRT_SIDES = 54

args = sys.argv[sys.argv.index("--") + 1 :]
source, out = args[0], args[1]
lights_out = args[2] if len(args) > 2 else None

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=source)


def bounds(objects):
    points = [o.matrix_world @ Vector(c) for o in objects for c in o.bound_box]
    low = Vector(tuple(min(p[i] for p in points) for i in range(3)))
    high = Vector(tuple(max(p[i] for p in points) for i in range(3)))
    return low, high


kept = []
for obj in list(bpy.data.objects):
    if obj.type == "MESH" and obj.name.startswith(KEEP):
        kept.append(obj)
pot_low, pot_high = bounds([o for o in bpy.data.objects if o.name.startswith(POT)])
for obj in kept:
    world = obj.matrix_world.copy()
    obj.parent = None
    obj.matrix_world = world
for obj in list(bpy.data.objects):
    if obj not in kept:
        bpy.data.objects.remove(obj, do_unlink=True)


def named(prefix):
    return [o for o in kept if o.name.startswith(prefix)]


def select(objects):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]


_, high = bounds(kept)
ground = pot_low.z + (high.z - pot_low.z) * SUNK
scale = HEIGHT_M / (high.z - ground)
shift = Vector((-(pot_low.x + pot_high.x) / 2, -(pot_low.y + pot_high.y) / 2, -ground))

for obj in kept:
    obj.location = (obj.location + shift) * scale
    obj.scale = obj.scale * scale

select(kept)
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

for obj in kept:
    for vert in obj.data.vertices:
        vert.co.x *= CROWN_WIDTH
        vert.co.y *= CROWN_WIDTH


def game_point(point):
    # Blender Z-up to the game's Y-up.
    return [round(point.x, 3), round(point.z, 3), round(-point.y, 3)]


def islands(obj):
    """Vertex indices of each connected piece of `obj`."""
    mesh = bmesh.new()
    mesh.from_mesh(obj.data)
    seen, found = set(), []
    for start in mesh.verts:
        if start.index in seen:
            continue
        island, stack = [], [start]
        seen.add(start.index)
        while stack:
            vert = stack.pop()
            island.append(vert.index)
            for edge in vert.link_edges:
                other = edge.other_vert(vert)
                if other.index not in seen:
                    seen.add(other.index)
                    stack.append(other)
        found.append(island)
    mesh.free()
    return found


def box_of(obj, indices):
    points = [obj.data.vertices[i].co for i in indices]
    low = Vector(tuple(min(p[i] for p in points) for i in range(3)))
    high = Vector(tuple(max(p[i] for p in points) for i in range(3)))
    return low, high


def ornaments(obj):
    """Islands of `obj` gathered into whole ornaments: pieces that sit together."""
    pieces = [(island, sum(box_of(obj, island), Vector()) / 2) for island in islands(obj)]
    groups = []
    while pieces:
        group, frontier = [], [pieces.pop()]
        while frontier:
            piece = frontier.pop()
            group.append(piece)
            near = [p for p in pieces if (p[1] - piece[1]).length < ORNAMENT_REACH]
            for p in near:
                pieces.remove(p)
            frontier.extend(near)
        groups.append([i for island, _ in group for i in island])
    return groups


def rescale(obj, indices, factor, anchor):
    for i in indices:
        co = obj.data.vertices[i].co
        obj.data.vertices[i].co = anchor + (co - anchor) * factor


lights = []
for obj in named("lights_lod1"):
    for island in islands(obj):
        lights.append(game_point(sum((obj.data.vertices[i].co for i in island), Vector()) / len(island)))

# The star is the ornament that reaches highest; the rest hang on the boughs.
hung = [(obj, group) for obj in named("ornaments_lod1") for group in ornaments(obj)]
star_obj, star_group = max(hung, key=lambda item: box_of(*item)[1].z)
for obj, group in hung:
    box_low, box_high = box_of(obj, group)
    if group is star_group:
        foot = Vector(((box_low.x + box_high.x) / 2, (box_low.y + box_high.y) / 2, box_low.z))
        rescale(obj, group, STAR_SCALE, foot)
    else:
        rescale(obj, group, ORNAMENT_SCALE, (box_low + box_high) / 2)
star_low, star_high = box_of(star_obj, star_group)
star = game_point((star_low + star_high) / 2)

# The star is a flat cut-out: a second one across it reads from every side.
star_mesh = bmesh.new()
star_mesh.from_mesh(star_obj.data)
chosen = set(star_group)
faces = [f for f in star_mesh.faces if all(v.index in chosen for v in f.verts)]
copy = bmesh.ops.duplicate(star_mesh, geom=faces)["geom"]
bmesh.ops.rotate(
    star_mesh,
    verts=[g for g in copy if isinstance(g, bmesh.types.BMVert)],
    cent=Vector(((star_low.x + star_high.x) / 2, (star_low.y + star_high.y) / 2, 0)),
    matrix=Matrix.Rotation(math.pi / 2, 3, "Z"),
)
star_mesh.to_mesh(star_obj.data)
star_mesh.free()

foliage = named("Tree_lod1")[0]

# The sunk trunk and whatever else ended up underground.
buried = bmesh.new()
buried.from_mesh(foliage.data)
bmesh.ops.delete(
    buried, geom=[f for f in buried.faces if all(v.co.z < 0 for v in f.verts)], context="FACES"
)
for vert in buried.verts:
    vert.co.z = max(vert.co.z, -0.05)
buried.to_mesh(foliage.data)
buried.free()


def crown_radius(height):
    """How far the boughs reach at `height`."""
    reach = sorted(
        math.hypot(v.co.x, v.co.y) for v in foliage.data.vertices if abs(v.co.z - height) < 0.25
    )
    return reach[int(len(reach) * 0.9)] if reach else 0.1


def linear(hex_color):
    channels = [int(hex_color[i : i + 2], 16) / 255 for i in (1, 3, 5)]
    return [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in channels] + [1.0]


def plain_material(name, roughness, color=None):
    """One flat colour, or the mesh's vertex colours when `color` is None."""
    material = bpy.data.materials.new(name)
    if not material.node_tree:
        material.use_nodes = True
    shader = next(n for n in material.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    shader.inputs["Roughness"].default_value = roughness
    shader.inputs["Metallic"].default_value = 0.0
    if color:
        shader.inputs["Base Color"].default_value = linear(color)
    else:
        paint = material.node_tree.nodes.new("ShaderNodeVertexColor")
        paint.layer_name = "Color"
        material.node_tree.links.new(paint.outputs["Color"], shader.inputs["Base Color"])
    return material


def add_object(name, mesh, material):
    data = bpy.data.meshes.new(name)
    mesh.to_mesh(data)
    mesh.free()
    data.materials.append(material)
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    kept.append(obj)
    return obj


# Bare sprig cards let the far side show through: add turned copies of the
# boughs and put a dark body behind them.
for degrees, width, tall, lift in BOUGH_TURNS:
    turned = foliage.copy()
    turned.data = foliage.data.copy()
    bpy.context.scene.collection.objects.link(turned)
    turn = Matrix.Rotation(math.radians(degrees), 3, "Z")
    for vert in turned.data.vertices:
        vert.co = turn @ Vector((vert.co.x * width, vert.co.y * width, vert.co.z * tall + lift))
    kept.append(turned)

core = bmesh.new()
SIDES = 12
rings = [
    [
        core.verts.new((math.cos(a) * r, math.sin(a) * r, height))
        for a in (i * math.tau / SIDES for i in range(SIDES))
    ]
    for height, r in ((h, crown_radius(h) * CORE_FILL) for h in CORE_HEIGHTS)
]
tip = core.verts.new((0, 0, CORE_HEIGHTS[-1] + 0.4))
for lower, upper in zip(rings, rings[1:]):
    for i in range(SIDES):
        j = (i + 1) % SIDES
        core.faces.new((lower[i], lower[j], upper[j], upper[i]))
for i in range(SIDES):
    core.faces.new((rings[-1][i], rings[-1][(i + 1) % SIDES], tip))
for face in core.faces:
    face.smooth = True
add_object("core", core, plain_material("winter_tree_core", 1.0, CORE_COLOR))

decor = bmesh.new()
paint = decor.verts.layers.float_color.new("Color")
# The exporter paints only one material of a mesh, so the matte skirt is a mesh of its own.
cloth = bmesh.new()
cloth_paint = cloth.verts.layers.float_color.new("Color")


def tint(verts, color, layer=paint):
    for vert in verts:
        vert[layer] = linear(color)


def ball(centre, radius, color, squash=(1, 1, 1)):
    made = bmesh.ops.create_uvsphere(
        decor,
        u_segments=10,
        v_segments=6,
        radius=radius,
        matrix=Matrix.Translation(centre) @ Matrix.Diagonal((*squash, 1)),
    )["verts"]
    tint(made, color)
    for face in {f for v in made for f in v.link_faces}:
        face.smooth = True


def block(placement, size, color):
    made = bmesh.ops.create_cube(decor, size=1, matrix=placement @ Matrix.Diagonal((*size, 1)))["verts"]
    tint(made, color)


def drape(profile, color):
    """Cloth swept round the trunk along `profile`, rippled into folds."""
    swept = []
    for radius, height in profile:
        ring = []
        for i in range(SKIRT_SIDES):
            angle = i * math.tau / SKIRT_SIDES
            fold = math.sin(angle * SKIRT_FOLDS)
            reach = radius * (1 + 0.025 * fold)
            ring.append(
                cloth.verts.new((math.cos(angle) * reach, math.sin(angle) * reach, height * (1 + 0.3 * fold)))
            )
        tint(ring, color, cloth_paint)
        swept.append(ring)
    for inner, outer in zip(swept, swept[1:]):
        for i in range(SKIRT_SIDES):
            j = (i + 1) % SKIRT_SIDES
            cloth.faces.new((inner[i], outer[i], outer[j], inner[j])).smooth = True


rng = random.Random(12)
hung_count = 0
for ring, (height, count) in enumerate(BAUBLE_RINGS):
    for i in range(count):
        angle = ring * 2.4 + (i + rng.uniform(-0.2, 0.2)) * math.tau / count
        z = height + rng.uniform(-0.12, 0.12)
        reach = crown_radius(z) * BAUBLE_REACH
        radius = BAUBLE_RADII[(i + ring) % len(BAUBLE_RADII)] * (1 - 0.2 * ring / len(BAUBLE_RINGS))
        ball(
            Vector((math.cos(angle) * reach, math.sin(angle) * reach, z)),
            radius,
            BAUBLE_COLORS[(i + ring * 2) % len(BAUBLE_COLORS)],
        )
        hung_count += 1

RIBBON = 0.012
for width, depth, tall, around, distance, turn, lift, box_color, ribbon_color in GIFTS:
    angle = math.radians(around)
    placement = Matrix.Translation(
        (math.cos(angle) * distance, math.sin(angle) * distance, lift)
    ) @ Matrix.Rotation(math.radians(turn), 4, "Z")
    band = 0.16 * min(width, depth)
    block(placement @ Matrix.Translation((0, 0, tall / 2)), (width, depth, tall), box_color)
    for size in ((width + 2 * RIBBON, band, tall + RIBBON), (band, depth + 2 * RIBBON, tall + RIBBON)):
        block(placement @ Matrix.Translation((0, 0, size[2] / 2)), size, ribbon_color)
    for side in (-1, 1):
        ball(
            placement @ Vector((side * band * 0.85, 0, tall + band * 0.45)),
            band * 0.8,
            ribbon_color,
            (1.25, 0.7, 0.6),
        )

drape(SKIRT_PROFILE, SKIRT_COLOR)
drape(TRIM_PROFILE, IVORY)

# Painted on their own meshes: joined in, every other part would carry vertex colours too.
painted = [
    add_object("winter_tree_decor", decor, plain_material("winter_tree_decor", 0.35)),
    add_object("winter_tree_skirt", cloth, plain_material("winter_tree_cloth", 0.95)),
]

select([obj for obj in kept if obj not in painted])
bpy.ops.object.join()
tree = bpy.context.view_layer.objects.active
tree.name = "winter_tree"
tree.data.name = "winter_tree"
select([tree, *painted])

for image in bpy.data.images:
    if image.size[0] > SMALL_TEXTURE and image.name != "Image_0":
        image.scale(SMALL_TEXTURE, SMALL_TEXTURE)

bpy.ops.export_scene.gltf(
    filepath=out,
    export_format="GLB",
    export_image_format="WEBP",
    export_image_quality=88,
    export_yup=True,
    export_apply=True,
    export_animations=False,
    export_cameras=False,
    export_lights=False,
    use_selection=True,
)

# Foliage cards sort badly as blended surfaces: cut them out instead.
with open(out, "rb") as handle:
    data = handle.read()
json_length = struct.unpack_from("<I", data, 12)[0]
document = json.loads(data[20 : 20 + json_length])
for material in document["materials"]:
    if material["name"] in CUTOUT:
        material["alphaMode"] = "MASK"
        material["alphaCutoff"] = FOLIAGE_CUTOFF
        material["doubleSided"] = True
    else:
        material.pop("alphaMode", None)
payload = json.dumps(document, separators=(",", ":")).encode()
payload += b" " * (-len(payload) % 4)
rest = data[20 + json_length :]
with open(out, "wb") as handle:
    handle.write(struct.pack("<4sII", b"glTF", 2, 12 + 8 + len(payload) + len(rest)))
    handle.write(struct.pack("<I4s", len(payload), b"JSON"))
    handle.write(payload)
    handle.write(rest)

low, high = bounds([tree, *painted])
triangles = sum(len(p.vertices) - 2 for obj in (tree, *painted) for p in obj.data.polygons)
print("RESULT size x=%.2f y=%.2f z=%.2f low=%.2f tris=%d materials=%d lights=%d baubles=%d star=%s" % (
    high.x - low.x, high.y - low.y, high.z - low.z, low.z, triangles, len(document["materials"]),
    len(lights), hung_count, star))
print("RESULT bulbs y=%.2f..%.2f reach=%.2f" % (
    min(p[1] for p in lights), max(p[1] for p in lights), max(math.hypot(p[0], p[2]) for p in lights)))
print("RESULT materials", [(m["name"], m.get("alphaMode", "OPAQUE")) for m in document["materials"]])
print("RESULT painted", [
    document["materials"][p["material"]]["name"]
    for m in document["meshes"] for p in m["primitives"] if "COLOR_0" in p["attributes"]])
if lights_out:
    with open(lights_out, "w") as handle:
        json.dump({"bulbs": lights, "star": star}, handle)
