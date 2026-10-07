"""Check the index/thumb skin web beneath both ranger gloves."""
import importlib.util
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('coverage', ROOT / 'tools/validate-tripo-glove-coverage.py')
coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coverage)
fit, io = coverage.fit, coverage.io
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/ranger_tripo_gloves_v1'


def main():
    poses_path = OUTPUT / 'validation-poses.json'
    poses = json.loads(poses_path.read_text())
    assert len(poses) == 175
    sides = []
    for side, sign in [('Right', -1), ('Left', 1)]:
        body, faces, dense = fit.body_surface(('hands', 'forearms'), sign)
        centers = body[faces].mean(1)
        root = fit.BONES[side + 'HandThumb1']
        opening = fit.BONES[side + 'HandThumb2'] * .85 + fit.BONES[side + 'HandThumb3'] * .15
        axis = io.unit(opening - root)
        web = fit.BONES[side + 'HandIndex1'] - root
        web = io.unit(web - axis * (web @ axis))
        along, across = (centers - opening) @ axis, (centers - opening) @ web
        selected = (along > .003) & (along < .014) & (across > .025) & (across < .050)
        selected &= np.linalg.norm(centers - [sign * .483, .914, .003], axis=1) < .023
        patch = faces[selected]
        assert len(patch) == (14 if side == 'Right' else 13), f'{side}: canonical index/thumb patch changed'
        result = coverage.check(OUTPUT / 'gloves_ranger.glb', body, dense, patch, poses, 'ranger_glove_' + side.lower())
        assert result['uncovered_skin_points'] == 0, result
        sides.append(dict(side=side, skin_triangles=patch.tolist(), **result))
    report = dict(date='2026-10-07', scope='Right 14 and left 13 canonical index/thumb web skin triangles, three barycentric samples per triangle in 175 actual animation poses; outward rays within 30mm',
                  base_sha256=fit.digest(ROOT / 'assets/modular_human_male_01/parts/fitted/base.glb'),
                  validation_poses_sha256=fit.digest(poses_path), poses=len(poses), sides=sides)
    (ROOT / 'doc/assets/modular-ranger-tripo-gloves-coverage-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps([{key: side[key] for key in ['side', 'sampled_skin_points', 'uncovered_skin_points']} for side in sides]))


if __name__ == '__main__':
    main()
