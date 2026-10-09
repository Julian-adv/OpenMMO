"""Validate actual runtime binding, rigid ornaments and sampled body clearance."""
import argparse
import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
OUTPUT = ROOT / 'assets/modular_human_male_01/caveman/tripo_top_v1'
REPORT = ROOT / 'doc/assets/modular-caveman-tripo-top-animation-v1.json'
FITTING = ROOT / 'doc/assets/modular-caveman-tripo-top-fitting-v1.json'


def skin(points, dense, matrices):
    homogeneous = np.column_stack([points, np.ones(len(points))])
    return np.einsum('nj,jab,nb->na', dense, matrices[:, :3], homogeneous)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skip-runtime', action='store_true')
    args = parser.parse_args()
    if not args.skip_runtime:
        subprocess.run(['node', 'tools/validate-tripo-rogue.mjs', '--directory', str(OUTPUT.relative_to(ROOT)), '--part', 'top_caveman', '--report', str(REPORT.relative_to(ROOT))], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    report = json.loads(REPORT.read_text())
    for source in report['sources']:
        assert fit.digest(ROOT / source['path']) == source['sha256']
    fitting = json.loads(FITTING.read_text())
    assert fit.digest(OUTPUT / 'top_caveman.glb') == fitting['validation']['sha256']
    doc, binary = fit.read_glb(OUTPUT / 'top_caveman.glb')
    prim = doc['meshes'][0]['primitives'][0]
    attrs = prim['attributes']
    points = fit.io.accessor(doc, binary, attrs['POSITION'])
    faces = fit.io.accessor(doc, binary, prim['indices']).reshape(-1, 3)
    joints = fit.io.accessor(doc, binary, attrs['JOINTS_0'])
    weights = fit.io.accessor(doc, binary, attrs['WEIGHTS_0'])
    dense = np.zeros((len(points), len(fit.NAMES)))
    np.add.at(dense, (np.arange(len(points))[:, None], joints), weights)
    unique = np.unique(np.round(points, 6), axis=0, return_index=True)[1]
    rigid_groups = {name: set(fitting['regions'][name + '_vertices']) for name in ['horn', 'ornament']}
    edges = np.unique(np.sort(np.concatenate([faces[:, [0, 1]], faces[:, [1, 2]], faces[:, [2, 0]]]), axis=1), axis=0)
    edges = edges[np.linalg.norm(points[edges[:, 0]] - points[edges[:, 1]], axis=1) > .001]
    rigid_edges = {name: np.array([edge for edge in edges if set(edge).issubset(group)]) for name, group in rigid_groups.items()}
    rest_lengths = {name: np.linalg.norm(points[e[:, 0]] - points[e[:, 1]], axis=1) for name, e in rigid_edges.items()}
    body, body_faces, body_weights = fit.body_surface(('torso', 'neck', 'upper_arms', 'forearms', 'head'))
    samples = json.loads((OUTPUT / 'validation-poses.json').read_text())
    results = []
    maximum_rigid_error = dict.fromkeys(rigid_groups, 0.)
    for index, pose in enumerate(samples):
        matrices = np.array(pose['matrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
        posed = skin(points, dense, matrices)
        for name, edges in rigid_edges.items():
            lengths = np.linalg.norm(posed[edges[:, 0]] - posed[edges[:, 1]], axis=1)
            maximum_rigid_error[name] = max(maximum_rigid_error[name], float(np.max(abs(lengths / rest_lengths[name] - 1))))
        posed_body = skin(body, body_weights, matrices)
        probes = np.concatenate([posed[unique], posed[faces].mean(1)])
        nearest, normals, _ = fit.nearest_surface(probes, (posed_body, body_faces, body_weights), candidates=64)
        signed = np.sum((probes - nearest) * normals, axis=1)
        worst = int(signed.argmin())
        results.append(dict(clip=pose['clip'], time=pose['time'], minimum_signed_distance_m=float(signed[worst]), samples_below_minus_3mm=int((signed < -.003).sum()), worst_probe=probes[worst].tolist()))
        if index % 13 == 12:
            current = results[-13:]
            print(pose['clip'], min(p['minimum_signed_distance_m'] for p in current), flush=True)
    assert max(maximum_rigid_error.values()) < 1e-5, maximum_rigid_error
    report['strain_region'] = 'Chest ornaments; separately validate the shoulder horn as a rigid group'
    for clip in report['clips']:
        clip['chest_ornament_strain_p95'] = clip.pop('leather_core_strain_p95')
    report['rigid_ornament_edge_error_maximum'] = maximum_rigid_error
    report['rigid_ornament_validation_passed'] = True
    report['body_surface_diagnostic'] = dict(
        method='Posed unique vertices and triangle centers versus nearest body triangles; signed normal distance is diagnostic, not an exact intersection test',
        body_regions=['torso', 'neck', 'upper_arms', 'forearms', 'head'],
        pose_count=len(results), samples=results,
        worst=min(results, key=lambda item: item['minimum_signed_distance_m']),
    )
    report['sources'].append(dict(path=str(FITTING.relative_to(ROOT)), sha256=fit.digest(FITTING)))
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print('Rigid ornaments:', maximum_rigid_error, flush=True)


if __name__ == '__main__':
    main()
