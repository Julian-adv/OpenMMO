"""Extract the fitted boot's upper rim and inner radius for caveman boot trouser hems."""
import importlib.util
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('boots', ROOT / 'tools/fit-tripo-caveman-boots.py')
boots = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boots)
fit, io = boots.fit, boots.io
source = boots.OUTPUT / 'boots_caveman.glb'
doc, binary = fit.read_glb(source)
primitive = doc['meshes'][0]['primitives'][0]
vertices = io.accessor(doc, binary, primitive['attributes']['POSITION']).astype(float)
faces = io.accessor(doc, binary, primitive['indices']).reshape(-1, 3)
triangles = vertices[faces]
origin = np.array([.1664, -.045])
a = triangles.reshape(-1, 3)
b = triangles[:, [1, 2, 0]].reshape(-1, 3)
profile = []
for angle in np.linspace(0, 2 * np.pi, 128, endpoint=False):
    direction = np.array([np.cos(angle), np.sin(angle)])
    normal = np.array([-direction[1], direction[0]])
    da, db = (a[:, [0, 2]] - origin) @ normal, (b[:, [0, 2]] - origin) @ normal
    mask = (da * db <= 0) & (np.abs(da - db) > 1e-9)
    t = da[mask] / (da[mask] - db[mask])
    hits = a[mask] + t[:, None] * (b[mask] - a[mask])
    radii = (hits[:, [0, 2]] - origin) @ direction
    hits = hits[(radii > .025) & (hits[:, 1] > .43)]
    assert len(hits)
    height = hits[:, 1].max()
    upper = hits[hits[:, 1] > height - .0015]
    radius = ((upper[:, [0, 2]] - origin) @ direction).min()
    profile.append([round(float(height), 8), round(float(radius), 8)])
record = dict(source=str(source.relative_to(ROOT)), sha256=fit.digest(source), origin=origin.tolist(),
              leftLeg=fit.NAMES.index('LeftLeg'), rightLeg=fit.NAMES.index('RightLeg'),
              overlap=.003, inset=.003, taper=.045, profile=profile)
target = ROOT / 'client/src/lib/data/cavemanBootCuff.json'
target.write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(dict(samples=len(profile), minimum_height=min(p[0] for p in profile),
                      maximum_height=max(p[0] for p in profile), output=str(target.relative_to(ROOT)))))
