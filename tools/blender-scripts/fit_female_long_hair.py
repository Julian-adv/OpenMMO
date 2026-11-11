"""Finish a long straight hair that tools/split-female-long-hair.py cut off its mannequin and placed on the body:
weld it, lift what dips under the body surface, recompute normals and export.

blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_long_hair.py -- --style side [--review dir]
"""
import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Euler, Matrix, Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
HAIR = ROOT / 'assets/modular_human_female_01/hair'
BODY = ROOT / 'assets/modular_human_female_01/faces/tripo_ranger_v1/base_ranger.glb'
FITS = ROOT / 'client/src/lib/data/femaleHairFits.json'
STYLES = {
    # hairline_drop: metres the front hairline slides down the forehead (user request 2026-11-11).
    # fit: the hair's key in femaleHairFits.json. tuck: settle the cut edge over the forehead onto the skin.
    'side': {'folder': 'tripo_long_side_v3', 'part': 'hair_female_long_side', 'fit': 'long', 'hairline_drop': .015,
             'tuck': True},
    'bangs': {'folder': 'tripo_long_bangs_v2', 'part': 'hair_female_long_bangs', 'fit': 'long_bangs', 'tuck': False,
              'level_fringe': True, 'square_fringe': True},
}
CLEARANCE = .003
parser = argparse.ArgumentParser()
parser.add_argument('--style', choices=STYLES, required=True)
parser.add_argument('--body', default=str(BODY))
parser.add_argument('--hairline-drop', type=float)
parser.add_argument('--review', help='Folder for review renders; nothing is exported')
args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:])
style = STYLES[args.style]
directory = HAIR / style['folder']
source = directory / 'fitted_v1/hair-split.glb'

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=args.body)
for action in list(bpy.data.actions):
    bpy.data.actions.remove(action)
for obj in bpy.data.objects:
    if obj.type == 'ARMATURE':
        obj.data.pose_position = 'REST'
bpy.context.view_layer.update()
depsgraph = bpy.context.evaluated_depsgraph_get()
points, polygons = [], []
for obj in [o for o in bpy.data.objects if o.type == 'MESH']:
    mesh = obj.evaluated_get(depsgraph).to_mesh()
    start = len(points)
    points += [obj.matrix_world @ v.co for v in mesh.vertices]
    polygons += [[start + i for i in p.vertices] for p in mesh.polygons]
surface = BVHTree.FromPolygons(points, polygons)
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(source))
hair = next(obj for obj in set(bpy.data.objects) - before if obj.type == 'MESH')
hair.data.transform(hair.matrix_world)
hair.matrix_world = Matrix.Identity(4)
hair.parent = None
bm = bmesh.new()
bm.from_mesh(hair.data)
bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6)
bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context='VERTS')
bm.to_mesh(hair.data)
bm.free()
hair.data.update()
vertices = hair.data.vertices
edges = [[] for _ in vertices]
for edge in hair.data.edges:
    a, b = edge.vertices
    edges[a].append(b)
    edges[b].append(a)


def depth(point):
    nearest, normal, _, _ = surface.find_nearest(point)
    return (point - nearest).dot(normal), normal


def smoothstep(low, high, value):
    t = max(0, min(1, (value - low) / (high - low)))
    return t * t * (3 - 2 * t)


# The hair over the forehead shifts down and forward along the forehead's slope, fading out towards the crown
# and the sides; the clearance pass below lifts whatever ends up under the skin.
drop = style.get('hairline_drop', 0) if args.hairline_drop is None else args.hairline_drop
front = [smoothstep(.04, -.06, v.co.y) * smoothstep(1.62, 1.74, v.co.z) for v in vertices]
for index, vertex in enumerate(vertices):
    vertex.co += Vector((0, -.4, -1)) * drop * front[index]

# The game moves the hair by the saved fit for this face (femaleHairFit.ts); the edge is matched to the skin
# where the hair ends up, then moved back. The fit is in three.js axes: right, up, front.
saved = json.loads(FITS.read_text())['ranger'].get(style['fit'], {})
PIVOT = Vector((0, 1.6218, -.0144))
game = (Matrix.Translation(PIVOT + Vector([saved.get(f'move-{axis}', 0) / 100 for axis in 'xyz']))
        @ Euler([math.radians(saved.get(f'rotate-{axis}', 0)) for axis in 'xyz']).to_matrix().to_4x4()
        @ Matrix.Diagonal([saved.get('scale', 100) * saved.get(f'scale-{axis}', 100) / 10000 for axis in 'xyz'] + [1])
        @ Matrix.Translation(-PIVOT))
axes = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
worn = axes @ game @ axes.inverted()
# Long hair takes the fit in full at head height and none below the shoulders (femaleHairFit.ts).
IDENTITY = Matrix.Identity(4)


def worn_at(height):
    weight = smoothstep(1.46, 1.60, height)
    return IDENTITY * (1 - weight) + worn * weight


placed = [worn_at(vertex.co.z) for vertex in vertices]
for vertex in vertices:
    vertex.co = placed[vertex.index] @ vertex.co

# The cut edge around the face is a staircase of triangle sides: it is smoothed along its own length.
bm = bmesh.new()
bm.from_mesh(hair.data)
rim = {}
for edge in bm.edges:
    if len(edge.link_faces) == 1:
        a, b = (vertex.index for vertex in edge.verts)
        rim.setdefault(a, []).append(b)
        rim.setdefault(b, []).append(a)
bm.free()
face_rim = {i: near for i, near in rim.items() if len(near) == 2 and vertices[i].co.z > 1.60 and vertices[i].co.y < .03}
for _ in range(8):
    moved = {i: (vertices[a].co + vertices[b].co) / 2 for i, (a, b) in face_rim.items()}
    for i, target in moved.items():
        vertices[i].co = vertices[i].co.lerp(target, .5)

# The fringe's cut line sat higher over one eyebrow than the other: at each distance from the centre the higher
# side comes down to the lower one, fading out over the 5 cm above the cut (user request 2026-11-11).
levelled = 0
if style.get('level_fringe'):
    STEP, REACH = .006, .05
    hem = {}
    for i in face_rim:
        point = vertices[i].co
        if point.y < -.05 and 1.64 < point.z < 1.76 and abs(point.x) < .075:
            key = (point.x > 0, int(abs(point.x) / STEP))
            hem[key] = min(hem.get(key, 9), point.z)
    columns = sorted({column for _, column in hem})
    lower = {}
    for side in (False, True):
        for column in columns:
            near = [hem[(s, c)] for s in (False, True) for c in (column - 1, column, column + 1) if (s, c) in hem]
            own = [hem[(side, c)] for c in (column - 1, column, column + 1) if (side, c) in hem]
            if own:
                lower[(side, column)] = (min(own), min(near))
    for vertex in vertices:
        point = vertex.co
        key = (point.x > 0, int(abs(point.x) / STEP))
        if key not in lower or point.y > -.03:
            continue
        own, target = lower[key]
        if own - target > .001 and point.z < own + REACH:
            point.z -= (own - target) * (1 - smoothstep(own, own + REACH, point.z))
            levelled += 1

# The fringe's cut line runs level out to the side locks and meets their inner edge at a right angle: hair that
# rounded the corner off moves out of the rectangle to its nearer side (user request 2026-11-11).
squared = 0
corner = set()
if style.get('square_fringe'):
    front_rim = [vertices[i].co for i in rim if vertices[i].co.y < -.02]
    cut = sorted(p.z for p in front_rim if abs(p.x) < .04 and 1.64 < p.z < 1.76)
    cut = cut[len(cut) // 2]
    for i in rim:
        point = vertices[i].co
        if point.y < -.04 and abs(point.z - cut) < .01:
            point.z = cut
    DROP, WEDGE = .045, .04
    lock_lead = {}
    for side in (-1, 1):
        band = [p for p in front_rim if p.x * side > 0 and cut - DROP < p.z < cut - DROP + .02]
        edge = max(p.x * side for p in band)
        lock_lead[side > 0] = min(p.y for p in band if p.x * side > edge - .004)
        for vertex in vertices:
            point = vertex.co
            across, below = edge - point.x * side, cut - point.z
            if point.y > .045 or across <= 0 or below <= 0 or point.x * side < .03:
                continue
            fade = 1 - smoothstep(DROP, DROP + .03, below)
            if below < across and below < DROP:
                point.z = cut
            else:
                point.x += side * across * fade
            squared += 1
            corner.add(vertex.index)
        # Whatever already stood on the side lock's inner plane in this band belongs to the same wall.
        for vertex in vertices:
            point = vertex.co
            if (point.y < .045 and abs(point.x * side - edge) < .009 and cut - DROP - .03 < point.z < cut + .003):
                corner.add(vertex.index)

# The cut edge over the forehead stood off the skin and showed the hair's underside from below: the edge and
# two rings behind it settle onto the forehead.
RIM_HEIGHT = .0015
tuck = [0.0] * len(vertices)
bm = bmesh.new()
bm.from_mesh(hair.data)
for edge in bm.edges:
    if len(edge.link_faces) == 1:
        for vertex in edge.verts:
            tuck[vertex.index] = style['tuck'] * smoothstep(1.68, 1.72, vertex.co.z) * smoothstep(.03, -.02, vertex.co.y)
bm.free()
for _ in range(2):
    tuck = [max([tuck[i]] + [tuck[j] * .55 for j in edges[i]]) for i in range(len(vertices))]
for index, vertex in enumerate(vertices):
    if tuck[index] > 0:
        nearest, normal, _, _ = surface.find_nearest(vertex.co)
        vertex.co = vertex.co.lerp(nearest + normal * RIM_HEIGHT, tuck[index])
tucked = sum(1 for value in tuck if value > 0)

lift = []
for vertex in vertices:
    distance, normal = depth(vertex.co)
    wanted = CLEARANCE + (RIM_HEIGHT - CLEARANCE) * tuck[vertex.index]
    lift.append(normal * (wanted - distance) if distance < wanted else Vector())
for _ in range(2):
    lift = [max([lift[i]] + [lift[j] * .5 for j in edges[i]], key=lambda v: v.length) for i in range(len(vertices))]
for index, vertex in enumerate(vertices):
    vertex.co += lift[index]
lifted = [value.length for value in lift if value.length > 1e-5]
for vertex in vertices:
    vertex.co = placed[vertex.index].inverted() @ vertex.co
# Triangles standing on that inner plane are a wall running back from the fringe to the temple, seen from the
# front as a pale wedge: they go. Leaning the wall outward instead brought the wedge back from other angles.
removed = 0
if corner:
    bm = bmesh.new()
    bm.from_mesh(hair.data)
    # Beside the eye the wall drew a pale wedge. On the wearer's left the whole wall goes there. On the right
    # that left a dark gap into the hair, so only the piece joining the fringe's end to the lock's front goes
    # and the wall behind the lock's front edge stays (user requests 2026-11-11).
    def wedge(face):
        points = [placed[vertex.index] @ vertex.co for vertex in face.verts]
        middle = sum(points, Vector()) / 3
        if middle.x > 0:
            return min(point.z for point in points) > cut - WEDGE
        return middle.z > cut - WEDGE and middle.y < lock_lead[False] + .004

    wall = [face for face in bm.faces if all(vertex.index in corner for vertex in face.verts) and wedge(face)]
    removed = len(wall)
    bmesh.ops.delete(bm, geom=wall, context='FACES')
    bm.to_mesh(hair.data)
    bm.free()
    hair.data.update()
    vertices = hair.data.vertices
hair.data.update()
for polygon in hair.data.polygons:
    polygon.use_smooth = True

depths = [(depth(v.co)[0], v.co.copy()) for v in vertices]
inside = sorted((d, p) for d, p in depths if d < -.001)
bounds = [[min(v.co[i] for v in vertices) for i in range(3)], [max(v.co[i] for v in vertices) for i in range(3)]]
report = {
    'tool': f'Blender {bpy.app.version_string}', 'source': str(source.relative_to(ROOT)),
    'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
    'body': str(Path(args.body).resolve().relative_to(ROOT)),
    'method': 'Coincident vertices welded, vertices closer than the clearance to the body lifted along the body '
              'normal, normals recomputed; UVs and triangles as split.',
    'hairline_drop': {'metres': drop, 'vertices_moved': sum(1 for value in front if value > 0) if drop else 0,
                      'direction_blender': [0, -.4, -1], 'falloff_blender': 'y .04 to -.06, z 1.62 to 1.74'},
    'game_fit_matched': saved, 'face_rim_smoothed_vertices': len(face_rim),
    'fringe_levelled_vertices': levelled, 'fringe_squared_vertices': squared, 'fringe_corner_triangles_removed': removed,
    'forehead_rim': {'height': RIM_HEIGHT, 'vertices_moved': tucked},
    'clearance': {'clearance': CLEARANCE, 'vertices_moved': len(lifted), 'maximum_displacement': max(lifted, default=0)},
    'triangles': len(hair.data.polygons), 'vertices': len(vertices), 'bounds_blender': bounds,
    'rest_vertices_inside_body_over_1mm': len(inside),
    'rest_deepest_inside_m': inside[0][0] if inside else 0,
}
print('FIT', json.dumps(report))
for d, p in inside[:8]:
    print('INSIDE', round(d, 4), [round(c, 3) for c in p])

if not args.review:
    out = directory / 'fitted_v1'
    out.mkdir(exist_ok=True)
    hair.name = style['part']
    hair['part_id'] = hair.name
    hair['rig_id'] = 'human_female_01_local_v1'
    bpy.ops.object.select_all(action='DESELECT')
    hair.select_set(True)
    bpy.context.view_layer.objects.active = hair
    bpy.ops.export_scene.gltf(filepath=str(out / 'hair-fitted-unskinned.glb'), export_format='GLB',
                              use_selection=True, export_tangents=True)
    (out / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
    sys.exit()

for vertex in vertices:
    vertex.co = worn_at(vertex.co.z) @ vertex.co
hair.data.update()
scene = bpy.context.scene
scene.render.engine = 'CYCLES'
scene.cycles.samples = 32
scene.cycles.use_denoising = True
try:
    preferences = bpy.context.preferences.addons['cycles'].preferences
    preferences.compute_device_type = 'OPTIX'
    preferences.get_devices()
    for device in preferences.devices:
        device.use = True
    scene.cycles.device = 'GPU'
except Exception:
    pass
scene.world = bpy.data.worlds.new('Review')
scene.world.use_nodes = True
scene.world.node_tree.nodes['Background'].inputs[0].default_value = (.5, .5, .55, 1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value = 1.6
center = Vector((0, 0, 1.52))
sun = bpy.data.objects.new('key', bpy.data.lights.new('key', 'SUN'))
sun.data.energy = 1.2
scene.collection.objects.link(sun)
camera = bpy.data.objects.new('camera', bpy.data.cameras.new('camera'))
camera.data.type, camera.data.ortho_scale = 'ORTHO', .75
scene.collection.objects.link(camera)
scene.camera = camera
scene.render.resolution_x, scene.render.resolution_y = 600, 750
scene.view_settings.view_transform = 'AgX'
for name, offset in [('front', (0, -5, 0)), ('angle', (3.5, -5, .6)), ('side', (5, 0, 0)), ('back', (0, 5, 0)),
                     ('angle2', (-3.5, -5, .6)), ('top', (0, -2, 5)), ('close', (-2.5, -5, .5)),
                     ('close2', (2.5, -5, 1.5)), ('closelow', (-.6, -5, -1.6)), ('brow', (0, -5, -.6)), ('persp', (0, 0, 0))]:
    camera.data.type = 'PERSP' if name in ('brow', 'persp') else 'ORTHO'
    camera.data.lens = 45
    camera.data.ortho_scale = .24 if name == 'brow' else .42 if name.startswith('close') else .75
    center = Vector((0, 0, 1.70 if name == 'brow' else 1.68 if name == 'persp' else 1.66 if name.startswith('close') else 1.52))
    camera.location = center + (Vector((0, -.42, -.02)) if name == 'brow' else Vector((.17, -.45, .0)) if name == 'persp'
                                else Vector(offset))
    camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
    sun.rotation_euler = (center - (camera.location + Vector((1, 0, 2)))).to_track_quat('-Z', 'Y').to_euler()
    scene.render.filepath = str(Path(args.review) / f'fit-{name}.png')
    bpy.ops.render.render(write_still=True)
