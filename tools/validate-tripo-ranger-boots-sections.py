"""Measure static boot clearance and exported ranger equipment budget."""
import importlib.util
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('boots', ROOT / 'tools/fit-tripo-ranger-boots.py')
boots = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boots)
fit, io = boots.fit, boots.io
parts = ROOT / 'assets/modular_human_male_01'
files = ['fitted/base.glb', 'fitted/hair_crop.glb', 'ranger/tripo_top_v4/top_ranger.glb',
         'ranger/tripo_pants_v1/pants_ranger.glb', 'ranger/tripo_boots_v1/boots_ranger.glb']
counts = []
for file in files:
    doc, binary = fit.read_glb(parts / file)
    count = sum(len(io.accessor(doc, binary, primitive['indices'])) // 3
                for mesh in doc['meshes'] for primitive in mesh['primitives'])
    counts.append(dict(path=str((parts / file).relative_to(ROOT)), sha256=fit.digest(parts / file), triangles=count))
doc, binary = fit.read_glb(parts / files[-1])
primitive = doc['meshes'][0]['primitives'][0]
points = io.accessor(doc, binary, primitive['attributes']['POSITION'])
faces = io.accessor(doc, binary, primitive['indices']).reshape(-1, 3)
body, indices, _ = fit.body_surface(('feet', 'ankles', 'legs'), 1)
skin = body[indices]
sections = []
for y in [.235, .35, .43, .46]:
    low, high = io.section_bounds(skin, np.array([0, y, 0]), np.array([0, 1, 0]),
                                  np.array([[1, 0, 0], [0, 0, 1]]))
    origin = (low + high) / 2
    ray_origin = [origin[0], y, origin[1]]
    gaps = []
    for angle in np.linspace(0, 2 * np.pi, 48, endpoint=False):
        direction = [np.cos(angle), 0, np.sin(angle)]
        inner = boots.surface.ray_surface(skin, ray_origin, direction)
        outer = boots.surface.ray_surface(points[faces], ray_origin, direction)
        assert inner is not None and outer is not None
        gaps.append(outer - inner)
    assert min(gaps) > 0
    sections.append(dict(height_m=y, rays=48, minimum_outer_boot_to_skin_gap_m=min(gaps),
                         maximum_outer_boot_to_skin_gap_m=max(gaps)))
record = dict(date='2026-10-07', exported_equipped_parts=counts,
              assembled_exported_triangles_including_hidden=sum(c['triangles'] for c in counts),
              sections=sections, pants_cuff_profile='client/src/lib/data/rangerBootCuff.json', skin_cut_height_m=.43,
              boot_top_height_m=float(points[:, 1].max()),
              scope='Static radial outer-surface clearance on actual canonical body; 48 rays per section. '
                    'Does not certify wall thickness or animated intersections. Runtime trims preserve original exported files.')
(ROOT / 'doc/assets/modular-ranger-tripo-boots-sections-v1.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps(record))
