"""Extract the fitted ranger cuff sections for sleeve tucking."""
import importlib.util
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('gloves', ROOT / 'tools/fit-tripo-ranger-gloves.py')
gloves = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gloves)
source = gloves.OUTPUT / 'gloves_ranger.glb'
doc, raw = gloves.fit.read_glb(source)
primitive = doc['meshes'][0]['primitives'][0]
points = gloves.io.accessor(doc, raw, primitive['attributes']['POSITION'])
faces = gloves.io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
wrist, axis, basis = gloves.frame()
angles = np.arange(64) * 2 * np.pi / 64
radial = np.column_stack([np.cos(angles), np.sin(angles)])
sections = []
for along in np.linspace(-.205, -.17, 8):
    center, radius = gloves.glove.wrist_section(points[faces], wrist + axis * along, axis, basis, radial, outermost=False)
    radius = np.minimum.reduce([radius, np.roll(radius, 1), np.roll(radius, -1)])
    sections.append(dict(along=round(float(along), 8), center=center.round(8).tolist(), radius=radius.round(8).tolist()))
record = dict(source=str(source.relative_to(ROOT)), sha256=gloves.fit.digest(source),
              wrist=wrist.tolist(), axis=axis.tolist(), basis=basis.tolist(),
              rightForeArm=gloves.fit.NAMES.index('RightForeArm'), leftForeArm=gloves.fit.NAMES.index('LeftForeArm'),
              taperStart=-.255, taperEnd=-.231, inset=.004, sections=sections)
(ROOT / 'client/src/lib/data/rangerGloveCuff.json').write_text(json.dumps(record, indent=2) + '\n')
print('Extracted eight cuff sections and 64 radial directions.')
