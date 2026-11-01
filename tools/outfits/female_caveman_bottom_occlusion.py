import hashlib
import json
import sys
from pathlib import Path

import numpy as np
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb
from lib.glb_accessors import accessor

BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
DIRECTORY = ROOT / 'assets/items/female_caveman_bottom/tripo_v1/fitted_v9'
BOTTOM = DIRECTORY / 'bottom_female_caveman.glb'
doc, binary = read_glb(BODY)
primitive = doc['meshes'][0]['primitives'][0]
points = accessor(doc, binary, primitive['attributes']['POSITION'])
faces = accessor(doc, binary, primitive['indices']).reshape(-1, 3)
_, welded = np.unique(np.round(points, 6), axis=0, return_inverse=True)
welded = welded.ravel()
edges = welded[faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)]
_, labels = connected_components(coo_matrix((np.ones(len(edges)), (edges[:, 0], edges[:, 1])),
                                            shape=(welded.max() + 1,) * 2), directed=False)
face_labels = labels[welded[faces[:, 0]]]
main = np.bincount(face_labels).argmax()
hidden = np.zeros(len(faces), dtype=bool)
for label in np.unique(face_labels):
    if label == main:
        continue
    component = face_labels == label
    bounds = points[np.unique(faces[component])]
    if bounds[:, 1].min() > .85 and bounds[:, 1].max() < 1.15:
        hidden |= component
assert hidden.any() and not np.any(hidden & (face_labels == main))
record = {
    'body': str(BODY.relative_to(ROOT)),
    'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
    'bottom': 'bottom_female_caveman',
    'bottom_sha256': hashlib.sha256(BOTTOM.read_bytes()).hexdigest(),
    'mesh_index': 0,
    'primitive_index': 0,
    'primitive_triangles': len(faces),
    'hide_triangle_indices': np.flatnonzero(hidden).tolist(),
    'underwear_triangles': int(hidden.sum()),
    'covered_skin_triangles': 0,
    'scope': 'Preview-only reversible mask of lower underwear island; skin is preserved. Motion and mixing remain unverified.',
}
(DIRECTORY / 'body-occlusion.json').write_text(json.dumps(record, indent=2) + '\n')
print(f"Lower underwear triangles hidden: {record['underwear_triangles']}")
