"""Fit one part of a segmented Tripo female outfit onto the female base rig.

blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_outfit.py -- --outfit ranger --part gloves [--keep-pending]
"""
import copy
import hashlib
import json
import subprocess
import sys
from datetime import date
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
from outfits.female_outfit_adjust import TO_GLTF, bake, sleeve_end, sleeve_end_total
from outfits.female_outfit_pose import ArmPose, rotation_between, smoothstep
from outfits.mesh_components import welded_islands
from outfits.skinning import deform, deform_dense

BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
RIG_ID = 'human_female_01_local_v1'
ARMS = ('LeftArm', 'LeftForeArm', 'RightArm', 'RightForeArm')
LEGS = ('LeftLeg', 'LeftFoot', 'LeftToe', 'RightLeg', 'RightFoot', 'RightToe')
FEET = ('LeftFoot', 'LeftToe', 'RightFoot', 'RightToe')
OUTFITS = {
    'priest': dict(
        source='tripo_segmented_v1/segmented.glb',
        sha256='2baf723d6fad2117159e6e369032711808f7a9a17b95b7819ffefbca7ae21bfe',
        scale=1.8, on_heels=False,
        parts={
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', hide_bones=('LeftHand', 'RightHand')),
            'top': dict(nodes=['top', 'tripo_part_25_top_fragment'], version='fitted_v1', hide_bones=ARMS, hide_underwear=(1.15, 1.55),
                        # Coverage hid neck skin under the open collar, leaving a hole; restore skin seen from outside.
                        restore_visible=True, cut_inside_glove=True, align_sleeves=True, hem_band=(.95, 1.15)),
            'bottom': dict(nodes=['skirt'], version='fitted_v1'),
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=LEGS),
            'helmet': dict(nodes=['helmet'], version='fitted_v1'),
        }),
    # Generated from a concept painted over the base in its 30° heel pose, so every part stands that much lifted.
    'ranger': dict(
        source='tripo_segmented_v1/original/model.glb',
        sha256='255e4ab93e253c9b63d1991600cb82973db8a0efaf7264ff366947df0a824c5f',
        # Tripo centred the model on its bounding box, which put the body 7 cm forward (head 8.3 cm, thighs 7.1 cm).
        scale=1.86, on_heels=True, offset_y=.07, snug_cover=True,
        parts={
            # Fingerless: the generated fingers are dropped and the base fingers stay visible.
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', drop_skin=True, hide_inside_cuff=True,
                           restore_visible=True),
            # The sleeve ends at the elbow, bare once the gloves come off, so it is aligned as one tube
            # and cut above its skin-coloured end band.
            'top': dict(nodes=['top'], version='fitted_v1', hide_bones=ARMS, hide_underwear=(1.15, 1.55),
                        restore_visible=True, cut_inside_glove=True, align_sleeves='rigid', cut_skin_band=True),
            'bottom': dict(nodes=['skirt'], version='fitted_v1', hide_underwear=(.8, 1.12)),
            # Below the knee: the shin above the boot rim stays bare.
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=FEET),
            'hair': dict(nodes=['hair'], version='fitted_v1', restore_visible=True),
        }),
    # Same heel-pose concept as the ranger; the top and skirt kept Tripo's segment names.
    'bard': dict(
        source='tripo_segmented_v1/original/model.glb',
        sha256='f256e239c69c6924c3e418a97c92f692c5ebe9f93b963c79750bc30807e5984d',
        scale=1.86, on_heels=True, snug_cover=True,
        rename={'tripo_part_0': 'top', 'tripo_part_2': 'skirt'},
        parts={
            # Tripo's fingers are kept: dropping them for the base fingers left shards and empty holes.
            'gloves': dict(nodes=['left_glove', 'right_glove'], version='fitted_v1', hide_bones=('LeftHand', 'RightHand'),
                           hide_inside_cuff=True),
            # The puffed sleeve is gathered into the bracer, so it is cut inside the glove cuff.
            'top': dict(nodes=['top'], version='fitted_v1', hide_bones=ARMS, hide_underwear=(1.15, 1.55),
                        restore_visible=True, cut_inside_glove=True, align_sleeves='rigid'),
            'bottom': dict(nodes=['skirt'], version='fitted_v1', hide_underwear=(.8, 1.12)),
            # Tripo made the stiletto 2.5 cm short of the sole plane. The idle toe bend left the extra gaps
            # (metres, measured in game) after stretching to the heel-pose sole. The full stretch looked too long,
            # so only half of it is used (user decision 2026-11-05).
            'boots': dict(nodes=['left_boots', 'right_boots'], version='fitted_v1', hide_bones=FEET,
                          extend_heel={'left': .0029, 'right': .0048, 'share': .5, 'upright': .09, 'lean': .005}),
        }),
}
COVER_RAY = .06
# Sleeve alignment fades in over this distance from the shoulder joint, leaving the shoulder pads in place.
SLEEVE_BLEND = (.03, .09)
# The sleeve is cut this far inside the glove cuff, toward the hand.
SLEEVE_CUT_INSET = .01
# A sleeve with `cut_skin_band` is cut this far above its skin-coloured end band.
SKIN_BAND_MARGIN = .003
# A top with `hem_band` (Blender z range searched) is cut along its painted belt band this far below the band's top.
HEM_CUT_DEPTH = .01
# Skin stays visible this far back inside the sleeve so its opening shows no hidden-skin edge; 1 cm showed above the glove cuff.
SLEEVE_SKIN_OVERLAP = .006
# Gloves hide forearm and hand skin this far past their cuff rim and this close to the glove.
CUFF_SKIN_INSET = .01
CUFF_SKIN_GAP = .008
# Shared with the caveman, rogue, barbarian and knight boots, so the baked animation variant is reused.
HEEL_DEGREES = 30
# The generated boots stand on a flat-height ankle. The shoe below the band moves down to the posed sole
# as one piece and the shin tube inside the band stretches; stretching the shoe itself sheared the arch.
ANKLE_BAND = (.18, .32)
# `extend_heel` stretches this much of the stiletto above its tip, from the back of the boot, down to the sole plane.
HEEL_BACK = .18
HEEL_POST = .06
# `upright` (stiletto height above its tip) turns the post vertical about its top; vertices this close to its axis belong to it.
HEEL_POST_RADIUS = .012
# No part is pushed out of the skin (user decision 2026-11-04): skin up to this deep outside a garment is hidden instead.
POKE_DEPTH = .04
SNUG_COVER = .003

argv = sys.argv[sys.argv.index('--') + 1:]
OUTFIT = argv[argv.index('--outfit') + 1]
PART = argv[argv.index('--part') + 1]
CONFIG = OUTFITS[OUTFIT]
PARTS = CONFIG['parts']
SPEC = PARTS[PART]
RIGID_SLEEVES = SPEC.get('align_sleeves') == 'rigid'
SCALE = CONFIG['scale']
DIRECTORY = ROOT / f'assets/items/female_{OUTFIT}_outfit'
SOURCE = DIRECTORY / CONFIG['source']
SOURCE_SHA256 = CONFIG['sha256']
PREVIEW_FIT = ROOT / f'doc/assets/female-{OUTFIT}-preview-fit.json'
PART_ID = f'{PART}_female_{OUTFIT}'
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
heel_matrices = joint_matrices.copy()
if PART == 'boots' or CONFIG['on_heels']:
    # heel_pose needs scipy, which Blender's Python lacks.
    script = ('import json, sys; sys.path.insert(0, "tools"); from pathlib import Path; from lib.glb import read_glb; '
              'from outfits.female_boot_pose import heel_pose; '
              f'm, c = heel_pose(*read_glb(Path("{BODY}")), degrees={HEEL_DEGREES}); '
              'print(json.dumps({"deformation": m.tolist(), "config": c}))')
    result = json.loads(subprocess.run([str(ROOT / '.venv/bin/python'), '-c', script], cwd=ROOT,
                                       check=True, capture_output=True, text=True).stdout)
    heel_matrices[joint_columns] = to_blender @ np.array(result['deformation']) @ to_blender.T
    if PART == 'boots':
        joint_matrices, heel = heel_matrices, result['config']
bp = deform_dense(rest_points, bw, joint_matrices)
heel_points = bp if PART == 'boots' else deform_dense(rest_points, bw, heel_matrices) if CONFIG['on_heels'] else rest_points
# Negative: how far the heel pose drops the sole below the floor.
foot_shift = float(heel_points[rest_points[:, 2] < .144, 2].min())
bones = {b.name: np.array(rig.matrix_world @ b.head_local) for b in rig.data.bones}
surface = BVHTree.FromPolygons([Vector(p) for p in bp], bf.tolist())

before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
imported = {CONFIG.get('rename', {}).get(o.name, o.name): o for o in set(bpy.data.objects) - before if o.type == 'MESH'}
placement = Matrix.Translation((0, CONFIG.get('offset_y', 0), foot_shift if CONFIG['on_heels'] else 0)) @ Matrix.Scale(SCALE, 4)
world = {n: np.array([placement @ o.matrix_world @ v.co for v in o.data.vertices]) for n, o in imported.items()}
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
    obj.data.transform(placement @ obj.matrix_world)
    obj.matrix_world = Matrix.Identity(4)
    obj.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
if len(parts) > 1:
    bpy.ops.object.join()
mesh_obj = parts[0]
mesh_obj.name = mesh_obj.data.name = PART_ID
mesh = mesh_obj.data


def base_colour_pixels(material):
    node = next(n for n in material.node_tree.nodes if n.type == 'TEX_IMAGE' and any(
        link.to_socket.name == 'Base Color' for link in n.outputs['Color'].links))
    image = node.image
    pixels = np.empty(len(image.pixels), dtype=np.float32)
    image.pixels.foreach_get(pixels)
    return pixels.reshape(image.size[1], image.size[0], -1)[:, :, :3]


def texel(image, u, v):
    return image[int(np.clip(v, 0, .999) * image.shape[0]), int(np.clip(u % 1, 0, .999) * image.shape[1])]


def component_sizes(positions, faces):
    """Faces in each face's welded component; Tripo splits vertices along UV seams."""
    islands = welded_islands(positions, faces)[2][faces[:, 0]]
    _, inverse, sizes = np.unique(islands, return_inverse=True, return_counts=True)
    return sizes[inverse.ravel()]


def skin_coloured(r, g, b):
    return r > .5 and r > g * 1.15 and g > b * 1.05


def drop_skin():
    """Delete the skin-coloured fingers out of a fingerless glove, then the loose fragments left around them.

    Only connected skin regions go: single faces on leather highlights read as skin and left holes.
    """
    pixels = [base_colour_pixels(m) for m in mesh.materials]
    uv = mesh.uv_layers.active.data
    edit = bmesh.new()
    edit.from_mesh(mesh)
    edit.faces.ensure_lookup_table()
    samples = np.array([(4, 1, 1), (1, 4, 1), (1, 1, 4), (1, 1, 1)], dtype=float) / 6
    is_skin = np.zeros(len(edit.faces), bool)
    for face in edit.faces:
        image = pixels[face.material_index]
        uvs = np.array([uv[loop.index].uv for loop in face.loops[:3]])
        is_skin[face.index] = np.mean([skin_coloured(*texel(image, u, v)) for u, v in samples @ uvs]) >= .5
    positions = np.array([v.co for v in edit.verts])
    corners = np.array([[v.index for v in f.verts] for f in edit.faces])
    skin_faces = np.flatnonzero(is_skin)
    drop = set(skin_faces[component_sizes(positions, corners[skin_faces]) >= 15].tolist())
    kept = np.array([f for f in range(len(corners)) if f not in drop])
    drop |= set(kept[component_sizes(positions, corners[kept]) < .02 * len(kept)].tolist())
    bmesh.ops.delete(edit, geom=[edit.faces[f] for f in drop], context='FACES')
    edit.to_mesh(mesh)
    edit.free()
    mesh.update()
    return len(drop)


dropped_skin = drop_skin() if SPEC.get('drop_skin') else 0
source = np.array([v.co for v in mesh.vertices])
faces = np.array([p.vertices[:] for p in mesh.polygons])


def forearm_hand_columns(side):
    return [i for i, n in enumerate(names) if n == side + 'ForeArm' or n.startswith(side + 'Hand')]


def arm_vertices(points):
    nearest = np.array([bw[bf[surface.find_nearest(Vector(p))[2]]].mean(0) for p in points])
    return {side: nearest[:, [i for i, n in enumerate(names)
                              if n in (side + 'Arm', side + 'ForeArm') or n.startswith(side + 'Hand')]].sum(1) > .5
            for side in ('Left', 'Right')}


points = pose(source, arm_vertices(source), rigid_hand=PART == 'gloves')
if heel and not CONFIG['on_heels']:
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


def align_sleeves_rigid(points):
    """Rotate and shift each sleeve as one tube so its end sits on the base arm.

    The sleeve turns about its slice at the shoulder blend so the slice centre at its end lands on the
    arm skin's; everything between follows linearly (user request 2026-11-05). The shoulder blend keeps
    the shoulder seam in place, and that slice moves front/back only, as in `align_sleeves`.
    """
    sleeve_shift = {}
    arms = arm_vertices(points)
    for side in ('Left', 'Right'):
        sleeve = arms[side]
        columns = [i for i, n in enumerate(names) if n in (side + 'Arm', side + 'ForeArm')]
        skin = rest_points[bw[:, columns].sum(1) > .5]
        s_sleeve, _ = arm_param(points[sleeve], side)
        s_skin, _ = arm_param(skin, side)
        upper = np.linalg.norm(bones[side + 'ForeArm'] - bones[side + 'Arm'])
        stations = [SLEEVE_BLEND[1], s_sleeve.max() - .015]

        def centres(q, s):
            out = []
            for station in stations:
                start, end = ('Arm', 'ForeArm') if station < upper else ('ForeArm', 'Hand')
                axis = bones[side + end] - bones[side + start]
                axis /= np.linalg.norm(axis)
                across = np.cross(axis, [0, 1, 0])
                across /= np.linalg.norm(across)
                frame = np.array([across, np.cross(across, axis)])
                near = q[np.abs(s - station) < .01]
                coords = near @ frame.T
                out.append((coords.min(0) + coords.max(0)) / 2 @ frame + (near @ axis).mean() * axis)
            return np.array(out)

        sleeve_c, skin_c = centres(points[sleeve], s_sleeve), centres(skin, s_skin)
        pivot = sleeve_c[0]
        offset = np.array([0, (skin_c[0] - pivot)[1], 0])
        rotation = rotation_between(sleeve_c[1] - pivot, skin_c[1] - pivot - offset)
        q = points[sleeve]
        moved = (q - pivot) @ rotation.T + pivot + offset
        weight = smoothstep((s_sleeve - SLEEVE_BLEND[0]) / (SLEEVE_BLEND[1] - SLEEVE_BLEND[0]))[:, None]
        shift = weight * (moved - q)
        points[sleeve] = q + shift
        sleeve_shift[side] = {'stations_m': [float(v) for v in stations], 'rotation_deg': float(np.degrees(np.arccos(np.clip((np.trace(rotation) - 1) / 2, -1, 1)))),
                              'shoulder_offset_mm_y': float(offset[1] * 1000), 'vertices': int(sleeve.sum()),
                              'max_mm': float(np.linalg.norm(shift, axis=1).max() * 1000),
                              'end_residual_mm': float(np.linalg.norm(centres(points[sleeve], s_sleeve)[-1] - skin_c[-1]) * 1000)}
    return sleeve_shift


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


def cut_hem(edit):
    """Cut the top's lower edge along the top of its painted belt band, which the skirt's belt covers.

    The band is found from the texture, so the cut follows its curve (low at the front, high at the sides).
    Coordinates are shifted so that curve becomes a flat plane for bisect_plane, then shifted back.
    """
    uv = edit.loops.layers.uv.active
    pixels = [base_colour_pixels(m) for m in mesh.materials]
    hem = [f for f in edit.faces if any(SPEC['hem_band'][0] < v.co.z < SPEC['hem_band'][1] for v in f.verts)
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
            r, g, b = texel(image, u, v)
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

    torso = [v for v in edit.verts if SPEC['hem_band'][0] - .1 < v.co.z < SPEC['hem_band'][1] + .05 and abs(v.co.x) < .25]
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


def cuff_plane(glove, side):
    """Plane fitted to the glove cuff's top rim, its normal pointing toward the hand.

    The rim is slanted about 3 cm, so a plane square to the forearm would miss it on one side.
    """
    axis = bones[side + 'Hand'] - bones[side + 'ForeArm']
    axis /= np.linalg.norm(axis)
    along = (glove - bones[side + 'ForeArm']) @ axis
    across = glove - bones[side + 'ForeArm'] - along[:, None] * axis
    reference = np.cross(axis, [0, 1, 0])
    angle = np.arctan2(across @ np.cross(axis, reference), across @ reference)
    rim = along < along.min() + .04
    sectors = ((angle[rim] + np.pi) / (2 * np.pi) * 16).astype(int) % 16
    rim_points = np.array([glove[rim][sectors == k][np.argmin(along[rim][sectors == k])] for k in np.unique(sectors)])
    centre = rim_points.mean(0)
    normal = np.linalg.svd(rim_points - centre)[2][2]
    return centre, (normal if normal @ axis > 0 else -normal), axis


def cut_sleeves(points):
    """Cut each sleeve with a plane across the forearm just inside the glove cuff.

    The sleeve's end was ragged and its last band was skin-coloured; the glove hides the clean cut edge.
    """
    gloves_doc, gloves_raw = read_glb(DIRECTORY / 'gloves' / PARTS['gloves']['version'] / f'gloves_female_{OUTFIT}.glb')
    glove = np.vstack([accessor(gloves_doc, gloves_raw, primitive['attributes']['POSITION'])
                       for m in gloves_doc['meshes'] for primitive in m['primitives']]) @ GLTF_TO_BLENDER.T
    for vertex, point in zip(mesh.vertices, points):
        vertex.co = point
    edit = bmesh.new()
    edit.from_mesh(mesh)
    pixels = [base_colour_pixels(m) for m in mesh.materials] if SPEC.get('cut_skin_band') else []
    record = {}
    for side, sign in (('Left', 1), ('Right', -1)):
        centre, normal, axis = cuff_plane(glove[glove[:, 0] * sign > 0], side)
        cuff_top = float((centre - bones[side + 'ForeArm']) @ axis)
        cut = centre + normal * SLEEVE_CUT_INSET
        # Only this sleeve is cut; the plane also crosses the bodice.
        on_side = [v for v in edit.verts if v.co.x * sign > abs(bones[side + 'Arm'][0])]
        side_faces = [f for f in edit.faces if all(v.co.x * sign > abs(bones[side + 'Arm'][0]) for v in f.verts)]
        band = 0.
        if SPEC.get('cut_skin_band'):
            # A sleeve ending short of the cuff, bare once the gloves come off, is cut cleanly above its
            # skin-coloured end band; dropping those faces left a ragged end.
            uv = edit.loops.layers.uv.active
            depths = []
            for face in side_faces:
                image = pixels[face.material_index]
                u, v = np.mean([loop[uv].uv for loop in face.loops], axis=0)
                depth = (np.mean([np.array(c.co) for c in face.verts], axis=0) - cut) @ normal
                if depth > -.08 and skin_coloured(*texel(image, u, v)):
                    depths.append(depth)
            band = min(0., float(np.percentile(depths, 5)) - SKIN_BAND_MARGIN)
            cut = cut + normal * band
        geometry = on_side + list({e for f in side_faces for e in f.edges}) + side_faces
        result = bmesh.ops.bisect_plane(edit, geom=geometry, plane_co=Vector(cut), plane_no=Vector(normal))
        beyond = [v for v in on_side if v.is_valid and (np.array(v.co) - cut) @ normal > 1e-6]
        bmesh.ops.delete(edit, geom=beyond, context='VERTS')
        record[side] = {'plane': (cut.tolist(), normal.tolist()), 'cuff_top_from_elbow_m': cuff_top,
                        **({'skin_band_cut_inside_m': -band} if band else {}),
                        'rim_tilt_deg': float(np.degrees(np.arccos(np.clip(normal @ axis, -1, 1)))),
                        'removed_vertices': len(beyond), 'cut_vertices': sum(isinstance(e, bmesh.types.BMVert) for e in result['geom_cut'])}
    bmesh.ops.triangulate(edit, faces=edit.faces[:])
    hem = hem_height = None
    if 'hem_band' in SPEC:
        hem, hem_height = cut_hem(edit)
        bmesh.ops.triangulate(edit, faces=edit.faces[:])
    edit.to_mesh(mesh)
    edit.free()
    mesh.update()
    cut_points = np.array([v.co for v in mesh.vertices])
    cut_faces = np.array([poly.vertices[:] for poly in mesh.polygons])
    return cut_points, cut_faces, record, hem, hem_height




def extend_heels(posed, extra):
    """Turn each stiletto upright about its top, then stretch it toward its boot's lowest sole point, in the heel pose
    (standing feet are level there)."""
    posed = posed.copy()
    record = {}
    for side, sign in (('left', 1), ('right', -1)):
        foot = np.flatnonzero(np.sign(posed[:, 0]) == sign)
        y = posed[foot, 1]
        back = foot[y > y.max() - HEEL_BACK * (y.max() - y.min())]
        record[side] = {}
        if 'upright' in extra:
            height = extra['upright']
            tip = posed[back, 2].min()
            rise = posed[back, 2] - tip
            base = posed[back[rise < .004]].mean(0)
            pivot = posed[back[np.abs(rise - height) < .004]].mean(0)
            axis = base + np.clip(rise / height, 0, 1)[:, None] * (pivot - base)
            post = back[(rise < height + .01) & (np.linalg.norm(posed[back, :2] - axis[:, :2], axis=1) < HEEL_POST_RADIUS)]
            # Rigid turn about the top, fading out over the last centimetre so the heel cup is untouched.
            # `lean` sets the tip that far toward the toe (-y) of plumb.
            down = base - pivot
            target = np.array([0, -extra.get('lean', 0), down[2]])
            turn = np.cross(down, target)
            angle = np.arctan2(np.linalg.norm(turn), down @ target)
            turn /= np.linalg.norm(turn)
            weight = np.clip((pivot[2] - posed[post, 2]) / .01, 0, 1)
            for vertex, amount in zip(post, weight):
                a = angle * amount
                offset = posed[vertex] - pivot
                posed[vertex] = pivot + offset * np.cos(a) + np.cross(turn, offset) * np.sin(a) + turn * (turn @ offset) * (1 - np.cos(a))
            record[side].update(upright_degrees=float(np.degrees(angle)), upright_vertices=len(post))
        sole, tip = posed[foot, 2].min(), posed[back, 2].min()
        gap = (tip - sole + extra[side]) * extra.get('share', 1)
        if gap > 0:
            weight = np.clip(1 - (posed[back, 2] - tip) / HEEL_POST, 0, 1)
            posed[back, 2] -= weight * gap
            record[side].update(gap_m=float(gap), stretched_vertices=int((weight > 0).sum()))
    return posed, record


paired = PART in ('gloves', 'boots')
keep_pending = '--keep-pending' in argv
# Glove fits saved in the forearm frame need the elbow and wrist.
arms = {sign: (bones[side + 'ForeArm'] @ TO_GLTF.T, bones[side + 'Hand'] @ TO_GLTF.T)
        for side, sign in (('Left', 1), ('Right', -1))}
heel_extension = {}
sleeve_shift = {}
sleeve_cut = {}
hem_cut = hem_height = None
if heel:
    # Boots are skinned in the heel pose, then returned to bind space where preview adjustments apply.
    order, values = skin_weights(points)
    skinning = np.einsum('nk,nkij->nij', values, joint_matrices[order])
    points = np.linalg.solve(skinning, np.c_[points, np.ones(len(points))][..., None])[:, :3, 0]
    points, preview_fits = bake(PREVIEW_FIT, PART, points, paired, keep_pending=keep_pending, arms=arms)
    if SPEC.get('extend_heel'):
        homogeneous = np.c_[points, np.ones(len(points))][..., None]
        posed, heel_extension = extend_heels((skinning @ homogeneous)[:, :3, 0], SPEC['extend_heel'])
        points = np.linalg.solve(skinning, np.c_[posed, np.ones(len(posed))][..., None])[:, :3, 0]
else:
    # Weights come last so they are sampled where the adjusted, aligned vertices end up.
    # One-tube sleeves are aligned before the saved fits, which then move them as the preview showed.
    if RIGID_SLEEVES:
        sleeve_shift = align_sleeves_rigid(points)
    points, preview_fits = bake(PREVIEW_FIT, PART, points, paired, keep_pending=keep_pending, arms=arms)
    if SPEC.get('align_sleeves') is True:
        sleeve_shift = align_sleeves(points)
    if SPEC.get('cut_inside_glove'):
        points, faces, sleeve_cut, hem_cut, hem_height = cut_sleeves(points)
        first, welded, adjacency = topology(points, faces)
    if RIGID_SLEEVES:
        sleeve_ends = sleeve_end_total(preview_fits)
        joints = {sign: (bones[side + 'Arm'] @ TO_GLTF.T, bones[side + 'ForeArm'] @ TO_GLTF.T)
                  for side, sign in (('Left', 1), ('Right', -1))}
        points = sleeve_end(points, joints, sleeve_ends)
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


def snug(p):
    return CONFIG.get('snug_cover') and garment.find_nearest(Vector(p), SNUG_COVER)[0] is not None


def covers(p, n):
    return (garment.ray_cast(Vector(p + n * .001), Vector(n), COVER_RAY)[0] is not None
            or garment.ray_cast(Vector(p), Vector(-n), POKE_DEPTH)[0] is not None
            # Skin lying on the garment surface slips between both rays and z-fights through it.
            or snug(p))


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
    if snug(p):
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
cuff_planes = {}
if SPEC.get('hide_inside_cuff'):
    # Ray coverage left forearm and hand flecks poking through, so hide skin past the cuff rim that lies against
    # the glove; the fingers out of its open ends lie farther away and stay.
    for side, sign in (('Left', 1), ('Right', -1)):
        centre, normal, _ = cuff_plane(points[points[:, 0] * sign > 0], side)
        arm = (bw[:, forearm_hand_columns(side)].sum(1) > .5) & ((rest_points - centre) @ normal > CUFF_SKIN_INSET)
        arm[arm] = [garment.find_nearest(Vector(p), CUFF_SKIN_GAP)[0] is not None for p in rest_points[arm]]
        hidden |= arm[bf].all(1)
        cuff_planes[side] = {'centre_m': centre.round(4).tolist(), 'normal': normal.round(4).tolist()}
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
    # Arm skin only: the plane also passes the hips, and a side test bared the lower torso there.
    arm = bw[:, forearm_hand_columns(side)].sum(1)[bf].min(1) > .5
    hidden[past & arm] = False
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
bpy.ops.wm.save_as_mainfile(filepath=str(OUTPUT / f'female-{OUTFIT}-{PART}.blend'))


def percentiles(d):
    return {k: round(float(v) * 1000, 2) for k, v in zip(('p1', 'p5', 'median', 'p95'), np.percentile(d, [1, 5, 50, 95]))}


report = {'date': date.today().isoformat(), 'outfit': OUTFIT, 'part': PART, 'source_nodes': SPEC['nodes'],
          'source': {'path': str(SOURCE.relative_to(ROOT)), 'sha256': SOURCE_SHA256},
          'body': {'path': str(BODY.relative_to(ROOT)), 'sha256': body_sha256},
          'output': {'path': str(target.relative_to(ROOT)), 'sha256': digest(target)},
          'rig_id': RIG_ID, 'scale': SCALE, 'arm_pose': pose.record(),
          **({'heel_lift_m': -foot_shift} if CONFIG['on_heels'] else {}),
          **({'heel_pose_degrees': HEEL_DEGREES, 'leg_hidden_below_rim_m': rim_heights,
              **({} if CONFIG['on_heels'] else {'foot_stretch_m': foot_shift, 'ankle_band_m': ANKLE_BAND})} if heel else {}),
          'triangles': len(faces), 'vertices': len(points), **({'dropped_skin_faces': dropped_skin} if dropped_skin else {}), 'skin_distance_mm': percentiles(depths),
          'pushout': False, 'poke_depth_m': POKE_DEPTH,
          'hidden_body_triangles': int(hidden.sum()), 'restored_visible_triangles': restored, 'motion_poke_triangles': motion_pokes,
          'brush_triangles': {'hide': len(brush.get('hide', [])), 'show': len(brush.get('show', []))},
          'preview_fit_adjustments': preview_fits,
          **({'skin_hidden_past_cuff': cuff_planes} if cuff_planes else {}),
          **({'sleeve_axis_shift': sleeve_shift} if sleeve_shift else {}),
          **({'sleeve_end_offsets_cm': sleeve_ends} if RIGID_SLEEVES else {}),
          **({'sleeve_cut': sleeve_cut} if sleeve_cut else {}),
          **({'hem_cut': hem_cut} if hem_cut else {}),
          **({'heel_extension': heel_extension} if heel_extension else {}),
          'method': f'Uniform {SCALE} scale' + (', lowered by the heel pose lift' if CONFIG['on_heels'] else '') + ', forearm rotation about the elbow and rigid hand ICP from the generated body pose, '
                    'no push-out (covered and poking skin is hidden), barycentric skin weights from the nearest base skin smoothed over welded neighbours.',
          'status': 'Candidate; visual and animation review pending'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print('REPORT', json.dumps(report))
