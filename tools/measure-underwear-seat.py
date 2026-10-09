"""Sample the plate trouser rear curve for the underwear silhouette."""
import hashlib
import importlib.util
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('priest', ROOT / 'tools/fit-tripo-priest-pants.py')
priest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(priest)
reference = ROOT / 'assets/modular_human_male_01/parts/fitted/pants_plate.glb'
doc, raw = priest.fit.read_glb(reference)
triangles = []
for mesh in doc['meshes']:
    for primitive in mesh['primitives']:
        points = priest.io.accessor(doc, raw, primitive['attributes']['POSITION'])
        faces = priest.io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
        triangles.extend(points[faces])
rear = priest.rear_projector(np.asarray(triangles))
xs = np.linspace(0, .2, 11)
ys = np.linspace(.76, 1.14, 39)
rows = []
for y in ys:
    row = []
    for x in xs:
        samples, weights = [], []
        for side in [-1, 1]:
            for dx, wx in [(-.012, 1), (0, 2), (.012, 1)]:
                for dy, wy in [(-.012, 1), (0, 2), (.012, 1)]:
                    depth = rear(np.array([side * x + dx, y + dy, 0]))
                    if depth is not None and depth < 0:
                        samples.append(depth)
                        weights.append(wx * wy)
        row.append(round(float(-.025 + .9 * (np.average(samples, weights=weights) + .025)), 7) if samples else None)
    rows.append(row)
profile = dict(source=dict(path=str(reference.relative_to(ROOT)), sha256=hashlib.sha256(reference.read_bytes()).hexdigest()),
               method='Symmetric plate rear depth; 12mm weighted smoothing; 90 percent depth about Z=-25mm',
               x_step=.02, y_min=.76, y_step=.01, depths=rows)
(ROOT / 'client/src/lib/data/underwearSeat.json').write_text(json.dumps(profile, indent=2) + '\n')
