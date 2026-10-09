"""Check skin coverage beneath Tripo hand accessories in sampled game poses."""
import argparse
import importlib.util
import json
from pathlib import Path

import numpy as np

from outfits.skinning import deform

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('glove_fit', ROOT / 'tools/fit-tripo-glove.py')
glove = importlib.util.module_from_spec(spec)
spec.loader.exec_module(glove)
fit, io = glove.fit, glove.io


def covered(origins, normals, triangles):
    ab, ac = triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]
    cross = np.cross(normals[:, None], ac)
    determinant = np.einsum('rti,ti->rt', cross, ab)
    inverse = np.divide(1, determinant, out=np.zeros_like(determinant), where=abs(determinant) > 1e-12)
    delta = origins[:, None] - triangles[:, 0]
    u = np.einsum('rti,rti->rt', delta, cross) * inverse
    cross = np.cross(delta, ab)
    v = np.einsum('ri,rti->rt', normals, cross) * inverse
    distance = np.einsum('ti,rti->rt', ac, cross) * inverse
    hits = (abs(determinant) > 1e-12) & (u >= 0) & (v >= 0) & (u + v <= 1) & (distance >= 0) & (distance < .030)
    return hits.any(1)


def check(path, body, skin_weights, patch, poses, part='glove_rogue_right'):
    used, patch = np.unique(patch, return_inverse=True)
    body, skin_weights, patch = body[used], skin_weights[used], patch.reshape(-1, 3)
    body_joints = np.argsort(skin_weights, axis=1)[:, -4:]
    skin_weights = np.take_along_axis(skin_weights, body_joints, axis=1)
    doc, binary = fit.read_glb(path)
    primitive = next(mesh for mesh in doc['meshes'] if mesh['name'] == part)['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(doc, binary, attrs['POSITION'])
    faces = io.accessor(doc, binary, primitive['indices']).reshape(-1, 3)
    joints = io.accessor(doc, binary, attrs['JOINTS_0'])
    skin = io.accessor(doc, binary, attrs['WEIGHTS_0'])
    bary = np.array([[.6, .2, .2], [.2, .6, .2], [.2, .2, .6]])
    missed, samples, clips = 0, 0, {}
    for pose in poses:
        matrices = np.asarray(pose['matrices']).reshape(len(fit.NAMES), 4, 4).transpose(0, 2, 1)
        triangles = deform(body, body_joints, skin_weights, matrices)[patch]
        centers = np.einsum('ki,nij->nkj', bary, triangles).reshape(-1, 3)
        normals = io.unit(np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]))
        hits = covered(centers, np.repeat(normals, len(bary), axis=0), deform(points, joints, skin, matrices)[faces])
        count = int((~hits).sum())
        missed += count
        samples += len(hits)
        clips[pose['clip']] = clips.get(pose['clip'], 0) + count
    return dict(sha256=fit.digest(path), sampled_skin_points=samples, uncovered_skin_points=missed, uncovered_by_clip=clips)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path)
    parser.add_argument('--part', choices=['glove', 'wrap'], default='glove')
    args = parser.parse_args()
    if args.part == 'wrap':
        body, faces, dense = fit.body_surface(('hands', 'forearms'), 1)
        axis = io.unit(fit.BONES['LeftForeArm'] - fit.BONES['LeftHand'])
        along = (body[faces] - fit.BONES['LeftHand']) @ axis
        patch = faces[(along.min(1) > .017) & (along.max(1) < .077)]
        assert len(patch) > 0
        output = ROOT / 'assets/modular_human_male_01/rogue/tripo_wrap_v1'
        part = 'wrap_rogue_left'
        scope = 'Canonical left wrist skin triangles fully inside the wrap band; three interior points per triangle, outward rays to wrap within 30mm'
    else:
        body, faces, dense = fit.body_surface(('hands',), -1)
        centers = body[faces].mean(1)
        root = fit.BONES['RightHandThumb1']
        opening = fit.BONES['RightHandThumb2'] * .85 + fit.BONES['RightHandThumb3'] * .15
        axis = io.unit(opening - root)
        web = fit.BONES['RightHandIndex1'] - root
        web = io.unit(web - axis * (web @ axis))
        along, across = (centers - opening) @ axis, (centers - opening) @ web
        selected = (along > .003) & (along < .014) & (across > .025) & (across < .050)
        selected &= np.linalg.norm(centers - [-.483, .914, .003], axis=1) < .023
        patch = faces[selected]
        assert len(patch) == 14, 'Canonical finger-web patch changed; inspect the selected skin region'
        output, part = glove.OUTPUT, 'glove_rogue_right'
        scope = '14 canonical skin triangles at the right index/thumb web; three interior points per triangle, outward rays to glove within 30mm'
    poses_path = output / 'validation-poses.json'
    poses = json.loads(poses_path.read_text())
    assert len(poses) == 91
    target = output / (part + '.glb')
    report = dict(date='2026-10-02', scope=scope,
        base_sha256=fit.digest(ROOT / 'assets/modular_human_male_01/fitted/base.glb'),
        validation_poses_sha256=fit.digest(poses_path), poses=len(poses), skin_triangles=patch.tolist(),
        after=check(target, body, dense, patch, poses, part))
    if args.before:
        report['before'] = check(args.before, body, dense, patch, poses, part)
        assert report['before']['uncovered_skin_points'] > 0
    assert report['after']['uncovered_skin_points'] == 0, report['after']
    (ROOT / f'doc/assets/modular-rogue-tripo-{args.part}-coverage-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: report[key] for key in ['poses', 'after', 'before'] if key in report}, indent=2))


if __name__ == '__main__':
    main()
