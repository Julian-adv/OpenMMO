"""Model the maid outfit's four garter straps on the female body.

Each strap lies on the thigh from under the skirt down onto the stocking's lace band and takes the thigh skin's
weights, so it moves with the leg and the stocking. The skirt swings on its own rig and cannot hold a strap's end.

blender -b --python-exit-code 1 --python tools/blender-scripts/build_female_maid_garters.py
"""
import copy
import hashlib
import json
import sys
from datetime import date
from pathlib import Path

import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.mesh_components import welded_islands

OUTPUT = ROOT / 'assets/items/female_maid_outfit/garters/fitted_v1'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
BOOTS = ROOT / 'assets/items/female_maid_outfit/boots/fitted_v1/boots_female_maid.glb'
ROWS = 13
WIDTH = .012
THICKNESS = .0018
# Height (m) the straps start at, under the skirt, and how far they run down onto the stocking's lace band.
TOP = .905
OVERLAP = .02
# Clearance (m) over the skin and the stocking.
LIFT = .003
# The clasp on the lace band: width, height, depth (m).
CLASP = (.017, .02, .004)
BLACK = [.012, .012, .012, 1]


def load(path):
    doc, raw = read_glb(path)
    primitive = doc['meshes'][0]['primitives'][0]
    attributes = primitive['attributes']
    points = accessor(doc, raw, attributes['POSITION']).astype(float)
    faces = accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    joints = accessor(doc, raw, attributes['JOINTS_0']).astype(int)
    weights = accessor(doc, raw, attributes['WEIGHTS_0']).astype(float)
    dense = np.zeros((len(points), len(doc['skins'][0]['joints'])))
    np.add.at(dense, (np.arange(len(points))[:, None], joints), weights)
    return doc, raw, points, faces, dense / dense.sum(1, keepdims=True)


def barycentric(point, triangle):
    a, b, c = triangle
    uv = np.linalg.lstsq(np.column_stack((b - a, c - a)), point - a, rcond=None)[0]
    result = np.clip([1 - uv.sum(), *uv], 0, 1)
    return result / result.sum()


def box(centre, size):
    corners = np.array([[x, y, z] for z in (-1, 1) for y in (-1, 1) for x in (-1, 1)]) * np.array(size) / 2
    faces = np.array([[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4],
                      [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]])
    return corners + centre, faces


_, _, bp, bf, bw = load(BODY)
_, _, labels = welded_islands(bp, bf)
skin = labels == np.bincount(labels).argmax()
boots_doc, boots_raw, xp, xf, _ = load(BOOTS)

points, faces, weights, anchors = [], [], [], []
for side, x_sign in (('left', 1), ('right', -1)):
    leg = skin & (bp[:, 0] * x_sign > .02) & (bp[:, 1] > .6) & (bp[:, 1] < 1.02)
    leg_faces = bf[leg[bf].all(1)]
    thigh = BVHTree.FromPolygons([Vector(p) for p in bp], leg_faces.tolist())
    stocking_faces = xf[(xp[xf][:, :, 0] * x_sign > 0).all(1)]
    stocking = BVHTree.FromPolygons([Vector(p) for p in xp], stocking_faces.tolist())
    rim = xp[np.unique(stocking_faces)][:, 1].max()
    for facing, z_sign in (('front', 1), ('back', -1)):
        rows, dense = [], []
        for height in np.linspace(TOP, rim - OVERLAP, ROWS):
            band = bp[leg & (np.abs(bp[:, 1] - height) < .015)]
            x = (band[:, 0].min() + band[:, 0].max()) / 2
            origin = Vector((x, height, z_sign * .4))
            hits = [surface.ray_cast(origin, Vector((0, 0, -z_sign)), .8)[0] for surface in (thigh, stocking)]
            z = max(hit[2] * z_sign for hit in hits if hit is not None) * z_sign
            rows.append([x, height, z + z_sign * (LIFT + THICKNESS / 2)])
            hit, _, face, _ = thigh.find_nearest(Vector((x, height, z)))
            dense.append(barycentric(np.array(hit), bp[leg_faces[face]]) @ bw[leg_faces[face]])
        rows, dense = np.array(rows), np.array(dense)
        start = sum(len(p) for p in points)
        offsets = np.array([[-WIDTH / 2, 0, z_sign * THICKNESS / 2], [WIDTH / 2, 0, z_sign * THICKNESS / 2],
                            [-WIDTH / 2, 0, -z_sign * THICKNESS / 2], [WIDTH / 2, 0, -z_sign * THICKNESS / 2]])
        strap = []
        for row in range(ROWS - 1):
            for a, b in ((0, 1), (1, 3), (3, 2), (2, 0)):
                v, q = row * 4 + a, row * 4 + b
                strap += [[v, q, q + 4], [v, q + 4, v + 4]]
        last = 4 * (ROWS - 1)
        strap = np.array(strap + [[0, 2, 3], [0, 3, 1], [last, last + 1, last + 3], [last, last + 3, last + 2]])
        points.append((rows[:, None] + offsets).reshape(-1, 3))
        faces.append((strap[:, ::-1] if z_sign > 0 else strap) + start)
        weights.append(np.repeat(dense, 4, axis=0))
        # The clasp sits on the lace band and moves as one piece with the strap's end.
        clasp, clasp_faces = box(rows[-1] + [0, CLASP[1] / 2 - .004, z_sign * THICKNESS], CLASP)
        faces.append(clasp_faces + start + ROWS * 4)
        points.append(clasp)
        weights.append(np.tile(dense[-1], (len(clasp), 1)))
        anchors.append({'name': f'{side}_{facing}', 'top': rows[0].round(4).tolist(), 'bottom': rows[-1].round(4).tolist(),
                        'stocking_rim_m': float(rim)})

points, faces, weights = np.concatenate(points), np.concatenate(faces), np.concatenate(weights)
# Outward normals: every piece is convex across its section, so faces are wound away from their own centre line.
corners = points[faces]
normal = np.cross(corners[:, 1] - corners[:, 0], corners[:, 2] - corners[:, 0])
vertex_normals = np.zeros_like(points)
for corner in range(3):
    np.add.at(vertex_normals, faces[:, corner], normal)
vertex_normals /= np.maximum(np.linalg.norm(vertex_normals, axis=1, keepdims=True), 1e-12)
joints = np.argsort(-weights, axis=1)[:, :4]
top = np.take_along_axis(weights, joints, axis=1)
top /= top.sum(1, keepdims=True)
joints[top == 0] = 0

doc = copy.deepcopy(boots_doc)
doc['accessors'], doc['bufferViews'] = [], []
binary = bytearray()
for key in ('images', 'textures', 'samplers'):
    doc.pop(key, None)
doc['skins'][0]['inverseBindMatrices'] = add_accessor(
    doc, binary, accessor(boots_doc, boots_raw, boots_doc['skins'][0]['inverseBindMatrices']), 'MAT4')
doc['materials'] = [{'name': 'maid_garter_black_satin',
                     'pbrMetallicRoughness': {'baseColorFactor': BLACK, 'metallicFactor': 0, 'roughnessFactor': .45}}]
doc['meshes'] = [{'name': 'garters_female_maid', 'primitives': [{
    'attributes': {'POSITION': add_accessor(doc, binary, points, 'VEC3'),
                   'NORMAL': add_accessor(doc, binary, vertex_normals, 'VEC3'),
                   'JOINTS_0': add_accessor(doc, binary, joints, 'VEC4', 5123),
                   'WEIGHTS_0': add_accessor(doc, binary, top, 'VEC4')},
    'indices': add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5123), 'material': 0}]}]
node = next(node for node in doc['nodes'] if 'mesh' in node)
node.update(name='garters_female_maid', mesh=0,
            extras={'rig_id': 'human_female_01_local_v1', 'part_id': 'garters_female_maid'})
doc['scenes'][0]['extras'] = {'rig_id': 'human_female_01_local_v1'}
doc['buffers'][0]['byteLength'] = len(binary)
OUTPUT.mkdir(parents=True, exist_ok=True)
target = OUTPUT / 'garters_female_maid.glb'
write_glb(target, doc, binary)
report = {'date': date.today().isoformat(), 'strap_count': 4, 'width_m': WIDTH, 'thickness_m': THICKNESS,
          'top_height_m': TOP, 'triangles': len(faces), 'vertices': len(points),
          'sources': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in (BODY, BOOTS)},
          'anchors': anchors, 'output_sha256': hashlib.sha256(target.read_bytes()).hexdigest(),
          'method': 'Four modelled ribbons with a clasp each, laid 3 mm over the thigh skin and the stocking top, '
                    'with the thigh skin weights under them. No generation.'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print('REPORT', json.dumps(report))
