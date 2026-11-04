"""Fit one female priest part from the segmented Tripo outfit onto the female base rig.

blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_priest.py -- --part gloves [--keep-pending]
"""
import copy
import hashlib
import json
import subprocess
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.female_priest_adjust import bake
from outfits.female_priest_pose import SCALE, ArmPose, smoothstep
from outfits.skinning import deform, deform_dense

DIRECTORY = ROOT / 'assets/items/female_priest_outfit'
SOURCE = DIRECTORY / 'tripo_segmented_v1/segmented.glb'
SOURCE_SHA256 = '2baf723d6fad2117159e6e369032711808f7a9a17b95b7819ffefbca7ae21bfe'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
PREVIEW_FIT = ROOT / 'doc/assets/female-priest-preview-fit.json'
RIG_ID = 'human_female_01_local_v1'

PARTS = {
    'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', hide_bones=('LeftHand', 'RightHand')),
    'top': dict(nodes=['top', 'tripo_part_25_top_fragment'], version='fitted_v1',
                hide_bones=('LeftArm', 'LeftForeArm', 'RightArm', 'RightForeArm'), hide_underwear=(1.15, 1.55),
                # Coverage hid neck skin under the open collar, leaving a hole; restore skin seen from outside.
                restore_visible=True, cut_inside_glove=True, align_sleeves=True),
    'bottom': dict(nodes=['skirt'], version='fitted_v1'),
    'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1',
                  hide_bones=('LeftLeg', 'LeftFoot', 'LeftToe', 'RightLeg', 'RightFoot', 'RightToe')),
    'helmet': dict(nodes=['helmet'], version='fitted_v1'),
}
COVER_RAY = .06
# Sleeve alignment fades in over this distance from the shoulder joint, leaving the shoulder pads in place.
SLEEVE_BLEND = (.03, .09)
# The sleeve is cut this far inside the glove cuff, toward the hand.
SLEEVE_CUT_INSET = .01
# The top's hem is cut along its painted belt band (Blender z range searched) this far below the band's top.
HEM_BAND = (.95, 1.15)
HEM_CUT_DEPTH = .01
# Skin stays visible this far back inside the sleeve so its opening shows no hidden-skin edge; 1 cm showed above the glove cuff.
SLEEVE_SKIN_OVERLAP = .006
# Shared with the caveman, rogue, barbarian and knight boots, so the baked animation variant is reused.
HEEL_DEGREES = 30
# The generated boots stand on a flat-height ankle. The shoe below the band moves down to the posed sole
# as one piece and the shin tube inside the band stretches; stretching the shoe itself sheared the arch.
ANKLE_BAND = (.18, .32)
# No part is pushed out of the skin (user decision 2026-11-04): skin up to this deep outside a garment is hidden instead.
POKE_DEPTH = .04

argv = sys.argv[sys.argv.index('--') + 1:]
PART = argv[argv.index('--part') + 1]
SPEC = PARTS[PART]
PART_ID = f'{PART}_female_priest'
OUTPUT = DIRECTORY / PART / SPEC['version']
OUTPUT.mkdir(parents=True, exist_ok=True)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


assert digest(SOURCE) == SOURCE_SHA256

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.unit_settings.system = 'METRIC'
scene.unit_settings.scale_length = 1
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
rig.animation_data_clear()
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.view_layer.update()
# The preview skins and masks glTF primitive 0 (the skin), so fit against it rather than Blender's merged mesh.
base, base_raw = read_glb(BODY)
body_primitive = base['meshes'][0]['primitives'][0]
GLTF_TO_BLENDER = np.array([[1, 0, 0], [0, 0, -1], [0, 1, 0]], dtype=float)
to_blender = np.eye(4)
to_blender[:3, :3] = GLTF_TO_BLENDER
rest_points = accessor(base, base_raw, body_primitive['attributes']['POSITION']) @ GLTF_TO_BLENDER.T
rest_normals = accessor(base, base_raw, body_primitive['attributes']['NORMAL']) @ GLTF_TO_BLENDER.T
bf = accessor(base, base_raw, body_primitive['indices']).reshape(-1, 3)
joint_names = [base['nodes'][j]['name'] for j in base['skins'][0]['joints']]
names = [b.name for b in rig.data.bones]
joint_columns = np.array([names.index(n) for n in joint_names])
bw = np.zeros((len(rest_points), len(names)))
np.add.at(bw, (np.arange(len(rest_points))[:, None], joint_columns[accessor(base, base_raw, body_primitive['attributes']['JOINTS_0']).astype(int)]),
          accessor(base, base_raw, body_primitive['attributes']['WEIGHTS_0']))
heel = None
joint_matrices = np.tile(np.eye(4), (len(names), 1, 1))
if PART == 'boots':
    # heel_pose needs scipy, which Blender's Python lacks.
    script = ('import json, sys; sys.path.insert(0, "tools"); from pathlib import Path; from lib.glb import read_glb; '
              'from outfits.female_boot_pose import heel_pose; '
              f'm, c = heel_pose(*read_glb(Path("{BODY}")), degrees={HEEL_DEGREES}); '
              'print(json.dumps({"deformation": m.tolist(), "config": c}))')
    result = json.loads(subprocess.run([str(ROOT / '.venv/bin/python'), '-c', script], cwd=ROOT,
                                       check=True, capture_output=True, text=True).stdout)
    deformation, heel = np.array(result['deformation']), result['config']
    joint_matrices[joint_columns] = to_blender @ deformation @ to_blender.T
bp = deform_dense(rest_points, bw, joint_matrices)
foot_shift = float(bp[rest_points[:, 2] < .144, 2].min())
bones = {b.name: np.array(rig.matrix_world @ b.head_local) for b in rig.data.bones}
surface = BVHTree.FromPolygons([Vector(p) for p in bp], bf.tolist())

before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
imported = {o.name: o for o in set(bpy.data.objects) - before if o.type == 'MESH'}
scale = Matrix.Scale(SCALE, 4)
world = {n: np.array([scale @ o.matrix_world @ v.co for v in o.data.vertices]) for n, o in imported.items()}
top = world['top']
pose = ArmPose(bones, bp, bf, bw, names,
               {'Left': np.vstack([top[top[:, 0] > .2], world['left_glove']]),
                'Right': np.vstack([top[top[:, 0] < -.2], world['right_glove']])},
               {'Left': world['left_glove'], 'Right': world['right_glove']})
for name, obj in list(imported.items()):
    if name not in SPEC['nodes']:
        bpy.data.objects.remove(obj)
parts = [imported[n] for n in SPEC['nodes']]

bpy.ops.object.select_all(action='DESELECT')
for obj in parts:
    obj.data.transform(scale @ obj.matrix_world)
    obj.matrix_world = Matrix.Identity(4)
    obj.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
if len(parts) > 1:
    bpy.ops.object.join()
mesh_obj = parts[0]
mesh_obj.name = mesh_obj.data.name = PART_ID
mesh = mesh_obj.data
source = np.array([v.co for v in mesh.vertices])
faces = np.array([p.vertices[:] for p in mesh.polygons])


def arm_vertices(points):
    nearest = np.array([bw[bf[surface.find_nearest(Vector(p))[2]]].mean(0) for p in points])
    return {side: nearest[:, [i for i, n in enumerate(names)
                              if n in (side + 'Arm', side + 'ForeArm') or n.startswith(side + 'Hand')]].sum(1) > .5
            for side in ('Left', 'Right')}


points = pose(source, arm_vertices(source), rigid_hand=PART == 'gloves')
if heel:
    points[:, 2] += foot_shift * (1 - smoothstep((points[:, 2] - ANKLE_BAND[0]) / (ANKLE_BAND[1] - ANKLE_BAND[0])))

def topology(positions, faces):
    _, first, welded = np.unique(np.round(positions, 6), axis=0, return_index=True, return_inverse=True)
    welded = welded.ravel()
    edges = np.unique(np.sort(welded[faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)], axis=1), axis=0)
    adjacency = [[] for _ in first]
    for a, b in edges:
        if a != b:
            adjacency[a].append(int(b))
            adjacency[b].append(int(a))
    return first, welded, adjacency


first, welded, adjacency = topology(source, faces)


def smooth(values, keep, passes):
    for _ in range(passes):
        values = np.array([keep * values[i] + (1 - keep) * values[n].mean(0) if n else values[i]
                           for i, n in enumerate(adjacency)])
    return values


def signed(point):
    hit, normal, _, _ = surface.find_nearest(Vector(point))
    return float(np.dot(point - np.array(hit), np.array(normal)))


depths = np.array([signed(p) for p in points])


def skin_weights(points):
    weights = np.zeros((len(points), len(names)))
    for i, point in enumerate(points):
        hit, _, face, _ = surface.find_nearest(Vector(point))
        a, b, c = bp[bf[face]]
        uv = np.linalg.lstsq(np.column_stack((b - a, c - a)), np.array(hit) - a, rcond=None)[0]
        bary = np.clip([1 - uv.sum(), *uv], 0, 1)
        weights[i] = (bary / bary.sum()) @ bw[bf[face]]
    weights = smooth(weights[first], .5, 6)[welded]
    for side, sign in (('Left', -1), ('Right', 1)):
        opposite = [j for j, n in enumerate(names) if n.startswith(side)]
        weights[np.ix_(points[:, 0] * sign > .02, opposite)] = 0
    order = np.argsort(weights, axis=1)[:, -4:][:, ::-1]
    values = np.take_along_axis(weights, order, axis=1)
    values[values < .01] = 0
    values /= values.sum(1, keepdims=True)
    order[values == 0] = 0
    return order, values


def arm_param(q, side):
    """Distance along the shoulder-elbow-wrist bones and the bone direction at each point."""
    joints = [bones[side + n] for n in ('Arm', 'ForeArm', 'Hand')]
    best = np.full(len(q), np.inf)
    s, axis = np.zeros(len(q)), np.zeros((len(q), 3))
    start = 0.
    for a, b in zip(joints, joints[1:]):
        length = np.linalg.norm(b - a)
        direction = (b - a) / length
        t = np.clip((q - a) @ direction, 0, length)
        distance = np.linalg.norm(q - (a + t[:, None] * direction), axis=1)
        closer = distance < best
        best[closer], s[closer], axis[closer] = distance[closer], start + t[closer], direction
        start += length
    return s, axis


def align_sleeves(points):
    sleeve_shift = {}
    # Shift each sleeve so its centre line follows the base arm's; shape and thickness stay.
    # The shoulder station keeps only front/back (Blender y): a sideways shift there opened gaps under the shoulder pads.
    # Elbow and wrist correct every direction across the arm.
    arms = arm_vertices(points)
    for side in ('Left', 'Right'):
        sleeve = arms[side]
        columns = [i for i, n in enumerate(names) if n in (side + 'Arm', side + 'ForeArm')]
        skin = rest_points[bw[:, columns].sum(1) > .5]
        s_sleeve, axis_sleeve = arm_param(points[sleeve], side)
        s_skin, _ = arm_param(skin, side)
        # Matching every 2 cm made the sleeve wavy, so only shoulder, elbow and wrist are matched, linear between.
        upper = np.linalg.norm(bones[side + 'ForeArm'] - bones[side + 'Arm'])
        lower = np.linalg.norm(bones[side + 'Hand'] - bones[side + 'ForeArm'])
        stations = [SLEEVE_BLEND[1], upper, upper + lower - .02]
        offsets = []
        for index, station in enumerate(stations):
            near_sleeve = np.abs(s_sleeve - station) < .03
            near_skin = np.abs(s_skin - station) < .03
            assert near_sleeve.sum() > 8 and near_skin.sum() > 8, f'{side} sleeve station {station:.3f} has too few points'
            start, end = [('Arm', 'ForeArm'), ('Arm', 'Hand'), ('ForeArm', 'Hand')][index]
            axis = bones[side + end] - bones[side + start]
            axis /= np.linalg.norm(axis)
            across = np.cross(axis, [0, 1, 0])
            across /= np.linalg.norm(across)
            frame = np.array([across, np.cross(across, axis)])

            def middle(q):
                # Middle of the slice's extent across the arm; a vertex mean leaned toward densely meshed spots.
                coords = q @ frame.T
                return (coords.min(0) + coords.max(0)) / 2 @ frame

            offset = middle(skin[near_skin]) - middle(points[sleeve][near_sleeve])
            if index == 0:
                offset = np.array([0, offset[1], 0])
            offsets.append(offset)
        offsets = np.array(offsets)
        shift = np.column_stack([np.interp(s_sleeve, stations, offsets[:, k]) for k in range(3)])
        shift -= (shift * axis_sleeve).sum(1, keepdims=True) * axis_sleeve
        shift *= smoothstep((s_sleeve - SLEEVE_BLEND[0]) / (SLEEVE_BLEND[1] - SLEEVE_BLEND[0]))[:, None]
        points[sleeve] += shift
        sleeve_shift[side] = {'stations_m': [float(v) for v in stations], 'offsets_mm_xyz': (offsets * 1000).round(1).tolist(),
                              'vertices': int(sleeve.sum()), 'max_mm': float(np.linalg.norm(shift, axis=1).max() * 1000),
                              'median_mm': float(np.median(np.linalg.norm(shift, axis=1)) * 1000)}
    return sleeve_shift


def base_colour_pixels(material):
    node = next(n for n in material.node_tree.nodes if n.type == 'TEX_IMAGE' and any(
        link.to_socket.name == 'Base Color' for link in n.outputs['Color'].links))
    image = node.image
    pixels = np.empty(len(image.pixels), dtype=np.float32)
    image.pixels.foreach_get(pixels)
    return pixels.reshape(image.size[1], image.size[0], -1)[:, :, :3]


def cut_hem(edit):
    """Cut the top's lower edge along the top of its painted belt band, which the skirt's belt covers.

    The band is found from the texture, so the cut follows its curve (low at the front, high at the sides).
    Coordinates are shifted so that curve becomes a flat plane for bisect_plane, then shifted back.
    """
    uv = edit.loops.layers.uv.active
    pixels = [base_colour_pixels(m) for m in mesh.materials]
    hem = [f for f in edit.faces if any(HEM_BAND[0] < v.co.z < HEM_BAND[1] for v in f.verts)
           and all(abs(v.co.x) < .2 for v in f.verts)]
    # The band is painted across large triangles, so sample the texture inside each one, not at its corners.
    weights = np.array([(i, j, 8 - i - j) for i in range(9) for j in range(9 - i)], dtype=float) / 8
    belt = []
    for face in hem:
        if len(face.loops) != 3:
            continue
        image = pixels[face.material_index]
        height, width = image.shape[:2]
        corners = np.array([loop.vert.co for loop in face.loops])
        uvs = np.array([loop[uv].uv for loop in face.loops])
        for position, (u, v) in zip(weights @ corners, weights @ uvs):
            r, g, b = image[int(np.clip(v, 0, .999) * height), int(np.clip(u % 1, 0, .999) * width)]
            if r > g * 1.15 and g > b * 1.1 and r < .75:
                belt.append(position)
    belt = np.array(belt)
    centre = belt.mean(0)
    sectors = 18

    def sector_of(co):
        return int((np.arctan2(co[0] - centre[0], co[1] - centre[1]) + np.pi) / (2 * np.pi) * sectors) % sectors

    tops = {}
    for co in belt:
        k = sector_of(co)
        tops.setdefault(k, []).append(co[2])
    known = sorted(tops)
    angles = (np.array(known) + .5) / sectors * 2 * np.pi - np.pi
    # Top of the band per direction; the 95th percentile ignores stray brown texels above it.
    heights = np.array([np.percentile(tops[k], 95) for k in known])

    def cut_height(co):
        angle = np.arctan2(co[0] - centre[0], co[1] - centre[1])
        return float(np.interp(angle, angles, heights, period=2 * np.pi)) - HEM_CUT_DEPTH

    torso = [v for v in edit.verts if HEM_BAND[0] - .1 < v.co.z < HEM_BAND[1] + .05 and abs(v.co.x) < .25]
    shift = {v: cut_height(v.co) for v in torso}
    for v in torso:
        v.co.z -= shift[v]
    torso_faces = [f for f in edit.faces if all(v in shift for v in f.verts)]
    geometry = torso + list({e for f in torso_faces for e in f.edges}) + torso_faces
    result = bmesh.ops.bisect_plane(edit, geom=geometry, plane_co=Vector((0, 0, 0)), plane_no=Vector((0, 0, 1)))
    new_verts = [e for e in result['geom_cut'] if isinstance(e, bmesh.types.BMVert)]
    for v in new_verts:
        shift[v] = cut_height(Vector((v.co.x, v.co.y, 0)))
    below = [v for v in shift if v.is_valid and v.co.z < -1e-6]
    for v in shift:
        if v.is_valid:
            v.co.z += shift[v]
    bmesh.ops.delete(edit, geom=below, context='VERTS')
    record = {'band_sectors': len(known), 'band_top_heights_m': [round(float(h), 4) for h in heights],
              'removed_vertices': len(below), 'cut_vertices': len(new_verts), 'depth_below_band_top_m': HEM_CUT_DEPTH}
    return record, cut_height


def cut_sleeves(points):
    """Cut each sleeve with a plane across the forearm just inside the glove cuff.

    The sleeve's end was ragged and its last band was skin-coloured; the glove hides the clean cut edge.
    """
    gloves_doc, gloves_raw = read_glb(DIRECTORY / 'gloves' / PARTS['gloves']['version'] / 'gloves_female_priest.glb')
    glove = np.vstack([accessor(gloves_doc, gloves_raw, primitive['attributes']['POSITION'])
                       for m in gloves_doc['meshes'] for primitive in m['primitives']]) @ GLTF_TO_BLENDER.T
    for vertex, point in zip(mesh.vertices, points):
        vertex.co = point
    edit = bmesh.new()
    edit.from_mesh(mesh)
    record = {}
    for side, sign in (('Left', 1), ('Right', -1)):
        axis = bones[side + 'Hand'] - bones[side + 'ForeArm']
        axis /= np.linalg.norm(axis)
        own = glove[glove[:, 0] * sign > 0]
        along = (own - bones[side + 'ForeArm']) @ axis
        # The cuff's top edge is slanted about 3 cm, so the cut plane is fitted to the rim and set inside it;
        # a cut square to the arm left skin showing on the rim's low side or the jagged end on its high side.
        across = own - bones[side + 'ForeArm'] - along[:, None] * axis
        reference = np.cross(axis, [0, 1, 0])
        angle = np.arctan2(across @ np.cross(axis, reference), across @ reference)
        rim = along < along.min() + .04
        sectors = ((angle[rim] + np.pi) / (2 * np.pi) * 16).astype(int) % 16
        rim_points = np.array([own[rim][sectors == k][np.argmin(along[rim][sectors == k])] for k in np.unique(sectors)])
        centre = rim_points.mean(0)
        normal = np.linalg.svd(rim_points - centre)[2][2]
        if normal @ axis < 0:
            normal = -normal
        cuff_top = float((centre - bones[side + 'ForeArm']) @ axis)
        cut = centre + normal * SLEEVE_CUT_INSET
        # Only this sleeve is cut; the plane also crosses the bodice.
        on_side = [v for v in edit.verts if v.co.x * sign > abs(bones[side + 'Arm'][0])]
        side_faces = [f for f in edit.faces if all(v.co.x * sign > abs(bones[side + 'Arm'][0]) for v in f.verts)]
        geometry = on_side + list({e for f in side_faces for e in f.edges}) + side_faces
        result = bmesh.ops.bisect_plane(edit, geom=geometry, plane_co=Vector(cut), plane_no=Vector(normal))
        beyond = [v for v in on_side if v.is_valid and (np.array(v.co) - cut) @ normal > 1e-6]
        bmesh.ops.delete(edit, geom=beyond, context='VERTS')
        record[side] = {'plane': (cut.tolist(), normal.tolist()), 'cuff_top_from_elbow_m': cuff_top,
                        'rim_tilt_deg': float(np.degrees(np.arccos(np.clip(normal @ axis, -1, 1)))),
                        'removed_vertices': len(beyond), 'cut_vertices': sum(isinstance(e, bmesh.types.BMVert) for e in result['geom_cut'])}
    bmesh.ops.triangulate(edit, faces=edit.faces[:])
    hem, hem_height = cut_hem(edit)
    bmesh.ops.triangulate(edit, faces=edit.faces[:])
    edit.to_mesh(mesh)
    edit.free()
    mesh.update()
    cut_points = np.array([v.co for v in mesh.vertices])
    cut_faces = np.array([poly.vertices[:] for poly in mesh.polygons])
    return cut_points, cut_faces, record, hem, hem_height


paired = PART in ('gloves', 'boots')
keep_pending = '--keep-pending' in argv
sleeve_shift = {}
sleeve_cut = {}
hem_cut = hem_height = None
if heel:
    # Boots are skinned in the heel pose, then returned to bind space where preview adjustments apply.
    order, values = skin_weights(points)
    skinning = np.einsum('nk,nkij->nij', values, joint_matrices[order])
    points = np.linalg.solve(skinning, np.c_[points, np.ones(len(points))][..., None])[:, :3, 0]
    points, preview_fits = bake(PREVIEW_FIT, PART, points, paired, keep_pending=keep_pending)
else:
    # Weights come last so they are sampled where the adjusted, aligned vertices end up.
    points, preview_fits = bake(PREVIEW_FIT, PART, points, paired, keep_pending=keep_pending)
    if SPEC.get('align_sleeves'):
        sleeve_shift = align_sleeves(points)
    if SPEC.get('cut_inside_glove'):
        points, faces, sleeve_cut, hem_cut, hem_height = cut_sleeves(points)
        first, welded, adjacency = topology(points, faces)
    order, values = skin_weights(points)

for vertex, point in zip(mesh.vertices, points):
    vertex.co = point
mesh.update()
face_normals = np.cross(points[faces[:, 1]] - points[faces[:, 0]], points[faces[:, 2]] - points[faces[:, 0]])
normal_sum = np.zeros((len(first), 3))
for k in range(3):
    np.add.at(normal_sum, welded[faces[:, k]], face_normals)
normal_sum /= np.maximum(np.linalg.norm(normal_sum, axis=1, keepdims=True), 1e-12)
mesh.normals_split_custom_set([normal_sum[welded[loop.vertex_index]] for loop in mesh.loops])
for key, data in (('_RIG_JOINTS', order), ('_RIG_WEIGHTS', values)):
    attribute = mesh.attributes.new(key, 'FLOAT_COLOR', 'POINT')
    attribute.data.foreach_set('color', data.astype(np.float32).ravel())

garment = BVHTree.FromPolygons([Vector(p) for p in points], faces.tolist())


def covers(p, n):
    return (garment.ray_cast(Vector(p + n * .001), Vector(n), COVER_RAY)[0] is not None
            or garment.ray_cast(Vector(p), Vector(-n), POKE_DEPTH)[0] is not None)


golden = np.pi * (3 - np.sqrt(5))
OPENING_CONE = .3
SNUG = .006
MOTION_CLIPS = ('idle1', 'walk', 'run', 'combat_idle', 'slash1')
MOTION_FRAMES = 6
VIEW_RAYS = np.array([[np.cos(golden * i) * np.sqrt(1 - z * z), np.sin(golden * i) * np.sqrt(1 - z * z), z]
                      for i, z in enumerate(np.linspace(-.95, .95, 48))])


def seen_through_opening(p, n):
    """Skin that some outward ray leaves without crossing the garment, and that is not in front of a garment face."""
    hit, normal, _, _ = garment.ray_cast(Vector(p), Vector(-n), POKE_DEPTH)
    if hit is not None and np.dot(np.array(normal), n) > 0:
        return False
    # Skin just under the garment pokes through once the idle pose moves it, so it stays hidden.
    if garment.ray_cast(Vector(p + n * .001), Vector(n), SNUG)[0] is not None:
        return False
    return any(garment.ray_cast(Vector(p + n * .001), Vector(d), .5)[0] is None
               for d in [n, *(d for d in VIEW_RAYS if np.dot(d, n) > OPENING_CONE)])


covered = np.array([covers(p, n) for p, n in zip(rest_points, rest_normals)])
# Erosion removed the scattered patches where unpushed skin pokes through, so the face centre is tested instead.
hidden = covered[bf].all(1)
for f in np.flatnonzero(hidden):
    normal = rest_normals[bf[f]].mean(0)
    hidden[f] = covers(rest_points[bf[f]].mean(0), normal / np.linalg.norm(normal))
restored = 0
if SPEC.get('restore_visible'):
    for f in np.flatnonzero(hidden):
        normal = rest_normals[bf[f]].mean(0)
        if seen_through_opening(rest_points[bf[f]].mean(0), normal / np.linalg.norm(normal)):
            hidden[f] = False
            restored += 1
if 'hide_underwear' in SPEC:
    _, welded_body = np.unique(np.round(rest_points, 5), axis=0, return_inverse=True)
    welded_body = welded_body.ravel()
    parent = np.arange(welded_body.max() + 1)

    def root(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for a, b, c in welded_body[bf]:
        parent[root(b)] = parent[root(c)] = root(a)
    islands = np.array([root(i) for i in welded_body])
    skin_island = np.bincount(islands[bf[:, 0]]).argmax()
    low, high = SPEC['hide_underwear']
    for island in np.unique(islands):
        z = rest_points[islands == island, 2]
        if island != skin_island and z.min() > low and z.max() < high:
            hidden |= (islands[bf] == island).all(1)
hide_columns = [i for i, n in enumerate(names) if n.startswith(SPEC.get('hide_bones', ()))]
if hide_columns:
    hidden |= bw[:, hide_columns].sum(1)[bf].min(1) > .5
rim_heights = {}
if heel:
    # Ray coverage missed knee patches, so hide each leg below the lowest point of its boot's top rim.
    for side, sign in (('Left', 1), ('Right', -1)):
        boot = points[points[:, 0] * sign > 0]
        top = boot[boot[:, 2] > boot[:, 2].max() - .08]
        offset = top[:, :2] - top[:, :2].mean(0)
        sector = ((np.arctan2(offset[:, 1], offset[:, 0]) + np.pi) / (2 * np.pi) * 24).astype(int) % 24
        rim = min(top[sector == k, 2].max() for k in np.unique(sector))
        columns = [i for i, n in enumerate(names) if n.startswith(tuple(side + b for b in ('UpLeg', 'Leg', 'Foot', 'Toe')))]
        leg = (bw[:, columns].sum(1) > .5) & (rest_points[:, 0] * sign > 0) & (rest_points[:, 2] < rim - .01)
        hidden |= leg[bf].all(1)
        rim_heights[side] = float(rim)
motion_pokes = 0
if not heel:
    # Skin covered at rest can poke through once body and garment deform differently, so pose both and
    # hide skin that lands in front of a garment face in any sampled frame.
    # Only skin the garment covers at rest; elsewhere hiding a frame's poke opened a hole in the other frames.
    centres = rest_points[bf].mean(1)
    face_normals = rest_normals[bf].mean(1)
    face_normals /= np.linalg.norm(face_normals, axis=1, keepdims=True)
    def under_or_against(f):
        if garment.ray_cast(Vector(centres[f] + face_normals[f] * .001), Vector(face_normals[f]), COVER_RAY)[0] is not None:
            return True
        # Skin lying just outside the bodice surface, which the normal rays slip past.
        near, near_normal, _, distance = garment.find_nearest(Vector(centres[f]))
        return distance < .01 and np.dot(centres[f] - np.array(near), np.array(near_normal)) > 0

    candidates = np.array([f for f in np.flatnonzero(~hidden) if under_or_against(f)], dtype=int)
    rig.animation_data_create()
    for clip in MOTION_CLIPS:
        action = bpy.data.actions[clip]
        rig.animation_data.action = action
        for frame in np.linspace(*action.frame_range, MOTION_FRAMES):
            scene.frame_set(int(frame), subframe=float(frame % 1))
            bpy.context.view_layer.update()
            frame_pose = np.array([np.array(rig.matrix_world @ rig.pose.bones[n].matrix @ rig.data.bones[n].matrix_local.inverted()
                                      @ rig.matrix_world.inverted()) for n in names])
            posed_garment = deform(points, order, values, frame_pose)
            posed_tree = BVHTree.FromPolygons([Vector(p) for p in posed_garment], faces.tolist())
            body_matrices = np.einsum('nk,kij->nij', bw, frame_pose)
            posed_body = np.einsum('nij,nj->ni', body_matrices[:, :3, :3], rest_points) + body_matrices[:, :3, 3]
            posed_normals = np.einsum('nij,nj->ni', body_matrices[:, :3, :3], rest_normals)
            for f in candidates[~hidden[candidates]]:
                centre = posed_body[bf[f]].mean(0)
                normal = posed_normals[bf[f]].mean(0)
                normal /= np.linalg.norm(normal)
                hit, face_normal, _, _ = posed_tree.ray_cast(Vector(centre), Vector(-normal), POKE_DEPTH)
                if hit is not None and np.dot(np.array(face_normal), normal) > 0:
                    hidden[f] = True
                    motion_pokes += 1
    rig.animation_data_clear()
    for bone in rig.pose.bones:
        bone.matrix_basis.identity()
    bpy.context.view_layer.update()

if hem_height:
    # Skin below the hem cut is bare once the skirt comes off; arms keep their own rule above.
    face_centres = rest_points[bf].mean(1)
    cut_heights = np.array([hem_height(c) for c in face_centres])
    below = (face_centres[:, 2] < cut_heights - SLEEVE_SKIN_OVERLAP) & (np.abs(face_centres[:, 0]) < .25)
    hidden[below] = False
for side, cut in sleeve_cut.items():
    # Skin past the cut is bare once the gloves come off, so only skin up to the cut stays hidden.
    origin, axis = (np.array(v) for v in cut.pop('plane'))
    past = (rest_points[bf].mean(1) - origin) @ axis > -SLEEVE_SKIN_OVERLAP
    arm_side = rest_points[bf].mean(1)[:, 0] * (1 if side == 'Left' else -1) > 0
    hidden[past & arm_side] = False
brush = json.loads(PREVIEW_FIT.read_text())['parts'].get(PART, {}).get('brush', {}) if PREVIEW_FIT.exists() else {}
hidden[brush.get('hide', [])] = True
hidden[brush.get('show', [])] = False

bpy.ops.object.select_all(action='DESELECT')
mesh_obj.select_set(True)
bpy.context.view_layer.objects.active = mesh_obj
intermediate = OUTPUT / 'geometry.glb'
bpy.ops.export_scene.gltf(filepath=str(intermediate), export_format='GLB', use_selection=True,
                          export_animations=False, export_skins=False, export_attributes=True, export_tangents=False)
doc, raw = read_glb(intermediate)
raw = bytearray(raw)
remap_order = np.array([joint_names.index(n) for n in names])
for primitive in doc['meshes'][0]['primitives']:
    attrs = primitive['attributes']
    joints = accessor(doc, raw, attrs.pop('_RIG_JOINTS')).astype(np.uint16)
    attrs['JOINTS_0'] = add_accessor(doc, raw, remap_order[joints], 'VEC4', 5123)
    attrs['WEIGHTS_0'] = add_accessor(doc, raw, accessor(doc, raw, attrs.pop('_RIG_WEIGHTS')), 'VEC4')
ids = base['skins'][0]['joints']
remap = {old: new for new, old in enumerate(ids)}
doc['nodes'] = [copy.deepcopy(base['nodes'][i]) for i in ids]
for node in doc['nodes']:
    if 'children' in node:
        node['children'] = [remap[i] for i in node['children'] if i in remap]
children = {i for node in doc['nodes'] for i in node.get('children', [])}
doc['nodes'].append({'name': 'Armature', 'children': [i for i in range(len(ids)) if i not in children]})
doc['nodes'].append({'name': PART_ID, 'mesh': 0, 'skin': 0, 'extras': {'rig_id': RIG_ID, 'part_id': PART_ID}})
skin = copy.deepcopy(base['skins'][0])
skin['joints'] = list(range(len(ids)))
if 'skeleton' in skin:
    skin['skeleton'] = remap[skin['skeleton']]
skin['inverseBindMatrices'] = add_accessor(doc, raw, accessor(base, base_raw, base['skins'][0]['inverseBindMatrices']), 'MAT4')
doc['skins'] = [skin]
doc['scenes'] = [{'nodes': [len(ids), len(ids) + 1], 'extras': {'rig_id': RIG_ID, **({'heel_pose': heel} if heel else {})}}]
doc['scene'] = 0
doc.pop('animations', None)
doc['buffers'] = [{'byteLength': len(raw)}]
target = OUTPUT / f'{PART_ID}.glb'
write_glb(target, doc, raw)
intermediate.unlink()

body_sha256 = digest(BODY)
mask = {'body': str(BODY.relative_to(ROOT)), 'body_sha256': body_sha256, 'mesh_index': 0, 'primitive_index': 0,
        'primitive_triangles': len(bf), 'hide_triangle_indices': np.flatnonzero(hidden).tolist(),
        'scope': f'Body triangles whose vertices all hit the {PART} within {COVER_RAY * 100:.0f} cm along their normals or lie within {POKE_DEPTH * 100:.0f} cm outside it, corners and centre'
                 + (f', plus triangles weighted over half to {", ".join(SPEC["hide_bones"])} bones.' if hide_columns else '.')}
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')

mesh_obj.modifiers.new('Female rig', 'ARMATURE').object = rig
mesh_obj.parent = rig
for group in list(mesh_obj.vertex_groups):
    mesh_obj.vertex_groups.remove(group)
for n in names:
    mesh_obj.vertex_groups.new(name=n)
for i in range(len(points)):
    for joint, weight in zip(order[i], values[i]):
        if weight:
            mesh_obj.vertex_groups[int(joint)].add([i], float(weight), 'REPLACE')
bpy.ops.file.pack_all()
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / f'female-priest-{PART}.blend'))


def percentiles(d):
    return {k: round(float(v) * 1000, 2) for k, v in zip(('p1', 'p5', 'median', 'p95'), np.percentile(d, [1, 5, 50, 95]))}


report = {'date': '2026-11-04', 'part': PART, 'source_nodes': SPEC['nodes'],
          'source': {'path': str(SOURCE.relative_to(ROOT)), 'sha256': SOURCE_SHA256},
          'body': {'path': str(BODY.relative_to(ROOT)), 'sha256': body_sha256},
          'output': {'path': str(target.relative_to(ROOT)), 'sha256': digest(target)},
          'rig_id': RIG_ID, 'scale': SCALE, 'arm_pose': pose.record(),
          **({'heel_pose_degrees': HEEL_DEGREES, 'foot_stretch_m': foot_shift, 'ankle_band_m': ANKLE_BAND,
              'leg_hidden_below_rim_m': rim_heights} if heel else {}),
          'triangles': len(faces), 'vertices': len(points), 'skin_distance_mm': percentiles(depths),
          'pushout': False, 'poke_depth_m': POKE_DEPTH,
          'hidden_body_triangles': int(hidden.sum()), 'restored_visible_triangles': restored, 'motion_poke_triangles': motion_pokes,
          'brush_triangles': {'hide': len(brush.get('hide', [])), 'show': len(brush.get('show', []))},
          'preview_fit_adjustments': preview_fits,
          **({'sleeve_axis_shift': sleeve_shift} if sleeve_shift else {}),
          **({'sleeve_cut': sleeve_cut} if sleeve_cut else {}),
          **({'hem_cut': hem_cut} if hem_cut else {}),
          'method': 'Uniform 1.8 scale, forearm rotation about the elbow and rigid hand ICP from the generated body pose, '
                    'no push-out (covered and poking skin is hidden), barycentric skin weights from the nearest base skin smoothed over welded neighbours.',
          'status': 'Candidate; visual and animation review pending'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print('REPORT', json.dumps(report))
