"""Verify posed pants-waist clearance against the final ranger top."""
import importlib.util
import argparse
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('ranger_fit', ROOT / 'tools/fit-tripo-ranger-top.py')
ranger = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ranger)
fit, io = ranger.fit, ranger.io


def load(path, waist=False, minimum_y=None):
    doc, binary = fit.read_glb(ROOT / path)
    result = []
    for node in doc['nodes']:
        if 'mesh' not in node or 'skin' not in node:
            continue
        if waist and node.get('extras', {}).get('region') != 'waist':
            continue
        for primitive in doc['meshes'][node['mesh']]['primitives']:
            attrs = primitive['attributes']
            vertices = io.accessor(doc, binary, attrs['POSITION'])
            faces = io.accessor(doc, binary, primitive['indices']).reshape(-1, 3)
            if minimum_y is not None:
                faces = faces[vertices[faces, 1].min(1) >= minimum_y]
            result.append((vertices, faces,
                           io.accessor(doc, binary, attrs['JOINTS_0']),
                           io.accessor(doc, binary, attrs['WEIGHTS_0'])))
    assert result
    return result


def posed(meshes, matrices):
    inverse = np.linalg.inv(matrices[fit.NAMES.index('Hips')])
    triangles = []
    for vertices, faces, joints, weights in meshes:
        points = np.column_stack([vertices, np.ones(len(vertices))])
        blend = (matrices[joints] * weights[:, :, None, None]).sum(1)
        local = np.einsum('nij,nj->ni', blend, points) @ inverse.T
        triangles.extend(local[faces, :3])
    return np.asarray(triangles)


def cross(a, b):
    return a[..., 0] * b[..., 1] - a[..., 1] * b[..., 0]


def radii(triangles, height):
    triangles = triangles[(triangles[:, :, 1].min(1) < height) & (triangles[:, :, 1].max(1) > height)]
    segments = []
    for triangle in triangles:
        hits = []
        for a, b in [(0, 1), (1, 2), (2, 0)]:
            if (triangle[a, 1] - height) * (triangle[b, 1] - height) < 0:
                point = triangle[a] + (triangle[b] - triangle[a]) * (height - triangle[a, 1]) / (triangle[b, 1] - triangle[a, 1])
                hits.append(point[[0, 2]] - [0, -.005])
        if len(hits) == 2:
            segments.append(hits)
    if not segments:
        return np.full(120, np.nan)
    segments = np.asarray(segments)
    start, edge = segments[:, 0], segments[:, 1] - segments[:, 0]
    angles = np.arange(120) * 2 * np.pi / 120
    direction = np.column_stack([np.cos(angles), np.sin(angles)])
    denominator = cross(direction[:, None, :], edge[None, :, :])
    safe = np.where(abs(denominator) > 1e-12, denominator, np.nan)
    radius = cross(start, edge)[None, :] / safe
    fraction = cross(start[None, :, :], direction[:, None, :]) / safe
    valid = (fraction >= 0) & (fraction <= 1) & (radius > 0)
    result = np.where(valid, radius, -np.inf).max(1)
    result[~np.isfinite(result)] = np.nan
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--revision', type=int, default=4)
    parser.add_argument('--pants', default='assets/modular_human_male_01/parts/fitted/pants_cloth.glb')
    parser.add_argument('--poses')
    parser.add_argument('--report')
    parser.add_argument('--heights', type=float, nargs='+', default=[1.08, 1.10, 1.12, 1.13, 1.14])
    args = parser.parse_args()
    revision = args.revision
    pants_path = args.pants
    cloth_waist = pants_path == 'assets/modular_human_male_01/parts/fitted/pants_cloth.glb'
    assert cloth_waist or args.report, 'Custom pants require a separate --report path'
    poses_path = args.poses or f'assets/modular_human_male_01/parts/ranger_tripo_top_v{revision}/validation-poses.json'
    poses = json.loads((ROOT / poses_path).read_text())
    pants = load(pants_path, waist=cloth_waist, minimum_y=None if cloth_waist else 1.0)
    sources = [pants_path, poses_path]
    revisions = []
    for candidate in [revision]:
        top_path = f'assets/modular_human_male_01/parts/ranger_tripo_top_v{candidate}/top_ranger.glb'
        sources.append(top_path)
        top, samples = load(top_path), []
        for pose in poses:
            matrices = np.asarray(pose['matrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
            inside, outside = posed(pants, matrices), posed(top, matrices)
            for height in args.heights:
                gap = radii(outside, height) - radii(inside, height)
                valid = np.isfinite(gap)
                assert valid.any()
                samples.append(dict(clip=pose['clip'], time=pose['time'], height_m=height,
                                    compared_rays=int(valid.sum()), unmatched_rays=int((~valid).sum()),
                                    minimum_clearance_m=float(gap[valid].min()),
                                    negative_rays=int((gap[valid] < 0).sum())))
        minimum = min(sample['minimum_clearance_m'] for sample in samples)
        if candidate == revision:
            assert minimum > .003, min(samples, key=lambda sample: sample['minimum_clearance_m'])
        revisions.append(dict(revision=candidate, minimum_clearance_m=minimum,
                              negative_rays=sum(sample['negative_rays'] for sample in samples), samples=samples))
    report = dict(date='2026-10-05', method='Outer section radii in inverse Hips deformation frame',
                  scope='Selected pants outer waist including accessories; custom pants use faces entirely above rest Y=1.0m so raised legs do not contaminate the waist measurement. Compare rays that intersect both surfaces. Unmatched rays include garment openings; this is not a full triangle intersection test.',
                  angles_per_plane=120, poses_per_revision=len(poses), planes_per_pose=len(args.heights),
                  sources=[dict(path=path, sha256=fit.digest(ROOT / path)) for path in sources], revisions=revisions)
    report_path = args.report or f'doc/assets/modular-ranger-tripo-top-waist-review-v{revision}.json'
    (ROOT / report_path).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps([dict(revision=row['revision'], minimum_clearance_m=row['minimum_clearance_m'],
                           negative_rays=row['negative_rays']) for row in revisions], indent=2))


if __name__ == '__main__':
    main()
