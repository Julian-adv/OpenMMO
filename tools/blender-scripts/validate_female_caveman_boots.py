import hashlib
import json
import os
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.mesh_components import welded_islands
from outfits.skinning import deform

OUTPUT = ROOT / 'assets/items/female_caveman_boots/tripo_v1' / f"fitted_{os.environ.get('FEMALE_CAVEMAN_BOOT_FIT_VERSION', 'v4')}"
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
BOOT = OUTPUT / 'boots_female_caveman.glb'


def mesh_data(doc, raw, index):
    p = doc['meshes'][index]['primitives'][0]
    a = p['attributes']
    return (accessor(doc, raw, a['POSITION']), accessor(doc, raw, p['indices']).reshape(-1, 3),
            accessor(doc, raw, a['JOINTS_0']).astype(int), accessor(doc, raw, a['WEIGHTS_0']))


base, body_raw = read_glb(BODY)
bp, bf, bj, bw = mesh_data(base, body_raw, 0)
doc, raw = read_glb(BOOT)
parts = [mesh_data(doc, raw, i) for i in range(2)]
morphs = [accessor(doc, raw, mesh['primitives'][0]['targets'][0]['POSITION']) for mesh in doc['meshes']]
_, _, labels = welded_islands(bp, bf)
skin = np.bincount(labels).argmax()
fitting = json.loads((OUTPUT / 'fitting.json').read_text())
cuff_height = fitting['cuff_height_m']
region = (labels == skin) & (bp[:, 1] > cuff_height-.10) & (bp[:, 1] < cuff_height+.07) & (np.abs(bp[:, 0]) < .30)
mask = json.loads((OUTPUT / 'body-occlusion.json').read_text())
hidden = np.zeros(len(bf), dtype=bool)
hidden[mask['hide_triangle_indices']] = True
samples = json.loads((OUTPUT / 'animation-samples.json').read_text())
report = {}
for sample in samples:
    matrices = np.array(sample['matrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
    body = deform(bp, bj, bw, matrices)
    gaps, visible = [], []
    for (points, faces, joints, weights), morph in zip(parts, morphs):
        side = -1 if points[0, 0] < 0 else 1
        ids = np.flatnonzero((region & (bp[:, 0]*side > 0))[bf].all(1))
        surface = BVHTree.FromPolygons([Vector(p) for p in body], bf[ids].tolist())
        posed = deform(points+morph if sample['clip'] == 'sit_idle' else points, joints, weights, matrices)
        vertex_ids = np.flatnonzero(points[:, 1] > cuff_height-.03)
        face_ids = np.flatnonzero(points[faces, 1].mean(1) > cuff_height-.03)
        queries = np.concatenate([posed[vertex_ids], posed[faces[face_ids]].mean(1)])
        for point in queries:
            hit, normal, face, _ = surface.find_nearest(Vector(point))
            distance = float(np.dot(point-np.array(hit), normal))
            gaps.append(distance)
            if not hidden[ids[face]]:
                visible.append(distance)
    result = {'phase': sample['phase'], 'minimum_full_cuff_gap_m': min(gaps),
              'minimum_visible_cuff_gap_m': min(visible),
              'visible_samples': len(visible), 'visible_deeper_than_1mm': int((np.array(visible) < -.001).sum())}
    name = sample['clip']
    if name not in report or result['minimum_visible_cuff_gap_m'] < report[name]['minimum_visible_cuff_gap_m']:
        report[name] = result
validation = {'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
              'boots_sha256': hashlib.sha256(BOOT.read_bytes()).hexdigest(), 'clips': report,
              'cuff_height_m': cuff_height,
              'scope': '61 actual runtime heel/ground-corrected frames per clip. Same-leg calf skin around the cuff; upper boot vertices and face centers in its top 30mm. Visible queries exclude nearest body triangles hidden by the boot mask. Lower covered skin and arm contact are outside this cuff check.'}
(OUTPUT / 'clearance-validation.json').write_text(json.dumps(validation, indent=2)+'\n')
print(json.dumps(validation, indent=2), flush=True)

assert all(report[name]['visible_deeper_than_1mm'] == 0 for name in ['idle1', 'walk', 'sit_idle']), 'Visible cuff penetration'
