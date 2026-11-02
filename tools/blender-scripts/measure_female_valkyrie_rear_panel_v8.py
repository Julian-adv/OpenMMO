import json
import sys
from pathlib import Path

import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.mesh_components import welded_islands
from outfits.skinning import deform

BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
BOTTOM = ROOT / 'assets/items/valkyrie_starter/tripo_bottom_v1'
POSES = Path('/tmp/valkyrie-bottom-waist-measure/idle1-poses.json')


def mesh(path):
    doc, binary = read_glb(path)
    primitive = doc['meshes'][0]['primitives'][0]
    attributes = primitive['attributes']
    return (accessor(doc, binary, attributes['POSITION']),
            accessor(doc, binary, attributes['JOINTS_0']).astype(int),
            accessor(doc, binary, attributes['WEIGHTS_0']),
            accessor(doc, binary, primitive['indices']).reshape(-1, 3))


body, body_joints, body_weights, body_faces = mesh(BODY)
before, joints, weights, faces = mesh(BOTTOM / 'fitted_v7/bottom_female_valkyrie.glb')
after, after_joints, after_weights, after_faces = mesh(BOTTOM / 'fitted_v8/bottom_female_valkyrie.glb')
assert np.array_equal(faces, after_faces) and np.array_equal(joints, after_joints) and np.array_equal(weights, after_weights)
_, _, labels = welded_islands(body, body_faces)
labels_present, counts = np.unique(labels, return_counts=True)
main = labels_present[np.argsort(counts)[-3:]]
body_faces = body_faces[np.isin(labels[body_faces], main).all(axis=1)]
centers = body[body_faces].mean(axis=1)
body_faces = body_faces[(centers[:, 1] > .86) & (centers[:, 1] < 1.20) &
                        (abs(centers[:, 0]) < .2) & (centers[:, 2] < .03)]
ids = np.arange(3534, 4217)
ids = ids[before[ids, 1] > 1.05]
poses = json.loads(POSES.read_text())['samples']
old_distances = []
new_distances = []
new_inside = 0
for sample in [0, 10, 20, 30, 40, 50, 60]:
    matrices = np.array(poses[sample]['matrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
    posed_body = deform(body, body_joints, body_weights, matrices)
    old_points = deform(before, joints, weights, matrices)
    new_points = deform(after, joints, weights, matrices)
    tree = BVHTree.FromPolygons([Vector(point) for point in posed_body], body_faces.tolist())
    for index in ids:
        hit, normal, _, _ = tree.find_nearest(Vector(old_points[index]))
        old_distance = (old_points[index] - np.array(hit)) @ np.array(normal)
        hit, normal, _, _ = tree.find_nearest(Vector(new_points[index]))
        new_distance = (new_points[index] - np.array(hit)) @ np.array(normal)
        old_distances.append(old_distance)
        new_distances.append(new_distance)
        new_inside += old_distance > .001 and new_distance < -.0002
report = {'samples': 7, 'upper_panel_vertices': len(ids),
          'median_before_mm': round(float(np.median(old_distances) * 1000), 2),
          'median_after_mm': round(float(np.median(new_distances) * 1000), 2),
          'minimum_before_mm': round(float(min(old_distances) * 1000), 2),
          'minimum_after_mm': round(float(min(new_distances) * 1000), 2),
          'new_inside_vertex_samples': int(new_inside),
          'scope': 'Signed nearest-body distance for selected upper panel vertices in seven idle poses; face intersections and other motions are not measured.'}
assert report['new_inside_vertex_samples'] == 0
(BOTTOM / 'fitted_v8/clearance-report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
