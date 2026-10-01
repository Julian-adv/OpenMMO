"""Measure the wrist wrap against deformed skin in every sampled game pose."""
import importlib.util
import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
spec = importlib.util.spec_from_file_location('rogue_fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)


def deform(points, joints, weights, matrices):
    homogeneous = np.column_stack([points, np.ones(len(points))])
    return np.einsum('nkij,nj,nk->ni', matrices[joints], homogeneous, weights)[:, :3]


def inside_skin(points, triangles, directions):
    votes = np.zeros(len(points), dtype=int)
    a, b, c = triangles[:, 0], triangles[:, 1], triangles[:, 2]
    e1, e2 = b - a, c - a
    offset = points[:, None] - a
    q = np.cross(offset, e1)
    distance_numerator = np.sum(e2 * q, axis=2)
    for direction in directions:
        h = np.cross(direction, e2)
        determinant = np.sum(e1 * h, axis=1)
        inverse = np.divide(1., determinant, out=np.zeros_like(determinant), where=abs(determinant) > 1e-12)
        u = np.sum(offset * h, axis=2) * inverse
        v = (q @ direction) * inverse
        distance = distance_numerator * inverse
        valid = (abs(determinant) > 1e-12) & (u >= 0) & (v >= 0) & (u + v <= 1) & (distance > 1e-7)
        for index in range(len(points)):
            hits = np.sort(distance[index, valid[index]])
            count = int(len(hits) > 0) + np.count_nonzero(np.diff(hits) > 1e-6)
            votes[index] += count % 2
    return votes >= 2


def main():
    candidate = fit.SELECTION['fitting_candidate']
    directory = ROOT / candidate['directory']
    doc, binary = fit.read_glb(directory / 'gloves_rogue.glb')
    mesh = next(mesh for mesh in doc['meshes'] if mesh['name'] == 'wrap_rogue_left')
    prim = mesh['primitives'][0]
    p, j, w = [fit.io.accessor(doc, binary, prim['attributes'][key]) for key in ['POSITION', 'JOINTS_0', 'WEIGHTS_0']]
    f = fit.io.accessor(doc, binary, prim['indices']).reshape(-1, 3)
    body, faces, dense = fit.body_surface(('hands', 'forearms'), 1)
    bj = np.argsort(dense, axis=1)[:, -4:]
    bw = np.take_along_axis(dense, bj, axis=1)
    wrist, elbow = fit.BONES['LeftHand'], fit.BONES['LeftForeArm']
    axis = fit.io.unit(elbow - wrist)
    across = fit.io.unit(np.cross(axis, [0, 0, 1]))
    forward = np.cross(axis, across)
    directions = np.array([across * np.cos(t) + forward * np.sin(t) for t in [.17, 1.31, 2.53]])
    h = (body - wrist) @ axis
    interval = json.loads((ROOT / candidate['report']).read_text())['wrist_wrap']['axial_interval_m']
    covered = np.intersect1d(np.unique(faces), np.where((h > interval[0] + .005) & (h < interval[1] - .005))[0])
    poses = json.loads((directory / 'validation-poses.json').read_text())
    results = []
    for pose in [dict(clip='rest', time=0, matrices=np.tile(np.eye(4).reshape(1, 16), (65, 1)).tolist()), *poses]:
        matrices = np.array(pose['matrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
        skin = deform(body, bj, bw, matrices)
        cloth = deform(p, j, w, matrices)
        q, normal, _ = fit.nearest_surface(cloth, (skin, faces, dense), candidates=128)
        clearance = np.linalg.norm(cloth - q, axis=1)
        rays = directions @ matrices[fit.NAMES.index('LeftForeArm'), :3, :3].T
        contained = inside_skin(cloth, skin[faces], rays)
        q, normal, _ = fit.nearest_surface(skin[covered], (cloth, f, np.zeros((len(cloth), 1))), candidates=128)
        inside = np.sum((skin[covered] - q) * normal, axis=1)
        results.append(dict(clip=pose['clip'], time=pose['time'], minimum_wrap_clearance_m=float(clearance.min()),
                            maximum_skin_signed_distance_m=float(inside.max()), inside_skin_vertices=int(contained.sum())))
    report = dict(poses=len(results), covered_skin_vertices=len(covered),
        minimum_wrap_clearance_m=min(p['minimum_wrap_clearance_m'] for p in results),
        inside_skin_vertices=sum(p['inside_skin_vertices'] for p in results),
        maximum_skin_signed_distance_m=max(p['maximum_skin_signed_distance_m'] for p in results), samples=results)
    print(json.dumps({k: v for k, v in report.items() if k != 'samples'}, indent=2))
    version = directory.name.rsplit('_', 1)[-1]
    output = ROOT / f'doc/assets/modular-rogue-wrap-review-{version}.json'
    report['method'] = 'Nearest surface distance plus majority parity of three forearm-plane rays; normals alone are insufficient at folded wrist skin'
    report['passed'] = report['minimum_wrap_clearance_m'] > .001 and report['maximum_skin_signed_distance_m'] < -.001 and report['inside_skin_vertices'] == 0
    output.write_text(json.dumps(report, indent=2) + '\n')
    assert report['passed'], 'Wrap intersects deformed skin'


if __name__ == '__main__':
    main()
