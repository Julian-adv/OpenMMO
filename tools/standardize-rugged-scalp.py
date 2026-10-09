import importlib.util
import json
import shutil
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
PARTS = ROOT / 'assets/modular_human_male_01'
DIRECTORY = PARTS / 'faces/tripo_rugged_v1'
SOURCE = DIRECTORY / 'face-rugged-before-standard-scalp.glb'
REPORT = ROOT / 'doc/assets/modular-rugged-standard-scalp.json'
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io


def canonical_scalp(points):
    vertices, faces, _ = fit.body_surface(('head',))
    triangles = vertices[faces]
    origin = np.array([0, 1.81, 0])
    directions = io.unit(points - origin)
    result = points.copy()
    edge1, edge2 = triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]
    offset = origin - triangles[:, 0]
    q = np.cross(offset, edge1)
    for start in range(0, len(points), 128):
        direction = directions[start:start + 128]
        p = np.cross(direction[:, None], edge2)
        determinant = np.einsum('fi,nfi->nf', edge1, p)
        inverse = np.zeros_like(determinant)
        np.divide(1, determinant, out=inverse, where=abs(determinant) > 1e-10)
        u = np.einsum('fi,nfi->nf', offset, p) * inverse
        v = np.einsum('ni,fi->nf', direction, q) * inverse
        distance = np.einsum('fi,fi->f', edge2, q)[None] * inverse
        valid = (abs(determinant) > 1e-10) & (u >= 0) & (v >= 0) & (u + v <= 1) & (distance > 0)
        distance = np.where(valid, distance, -np.inf).max(1)
        hit = np.isfinite(distance)
        rows = start + np.flatnonzero(hit)
        result[rows] = origin + directions[rows] * distance[hit, None]
    rear = 1 - io.smoothstep(points[:, 2] / .07)
    lower = io.smoothstep((points[:, 1] - 1.735) / .025)
    crown = io.smoothstep((points[:, 1] - 1.83) / .045)
    ears = io.smoothstep((np.abs(points[:, 0]) - .045) / .025) * (1 - io.smoothstep((points[:, 1] - 1.795) / .025))
    influence = np.maximum(rear * lower * (1 - ears), crown)
    return points + (result - points) * influence[:, None], influence


def main():
    if not SOURCE.exists():
        assert fit.digest(DIRECTORY / 'face_rugged.glb') == '69bf72e0170cf0770b7b9b555c5d73d67d5f163af13df3261c03fe3228c7a18a'
        shutil.copyfile(DIRECTORY / 'face_rugged.glb', SOURCE)
    source, raw = fit.read_glb(SOURCE)
    primitive = source['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(source, raw, attrs['POSITION'])
    faces = io.accessor(source, raw, primitive['indices']).reshape(-1, 3)
    normals = io.accessor(source, raw, attrs['NORMAL'])
    result, influence = canonical_scalp(points)
    result = result.astype(np.float32)
    smoothed = io.smooth_normals(result, faces)
    updated_normals = io.unit(normals * (1 - influence[:, None]) + smoothed * influence[:, None])
    untouched = influence == 0
    updated_normals[untouched] = normals[untouched]
    assert np.array_equal(result[untouched], points[untouched])
    cross = lambda p: np.cross(p[faces[:, 1]] - p[faces[:, 0]], p[faces[:, 2]] - p[faces[:, 0]])
    assert np.all(np.einsum('ij,ij->i', cross(points), cross(result)) > 0), 'Flipped head triangle'
    for name in ['face_rugged.glb', 'base_rugged.glb']:
        doc, binary = fit.read_glb(DIRECTORY / name)
        primitive = next(mesh for mesh in doc['meshes'] if mesh['name'] == 'face_rugged')['primitives'][0]
        binary = bytearray(binary)
        primitive['attributes']['POSITION'] = io.add_accessor(doc, binary, result, 'VEC3')
        primitive['attributes']['NORMAL'] = io.add_accessor(doc, binary, updated_normals, 'VEC3')
        fit.write_glb(DIRECTORY / name, doc, io.compact(doc, binary))
    report = {
        'date': '2026-10-05',
        'source': {'path': str(SOURCE.relative_to(ROOT)), 'sha256': fit.digest(SOURCE)},
        'canonical_head_sha256': fit.digest(PARTS / 'fitted/base.glb'),
        'shared_crop_sha256': fit.digest(PARTS / 'fitted/hair_crop.glb'),
        'shared_wavy_hair_sha256': fit.digest(PARTS / 'hair/tripo_wavy_v1/hair_wavy_bone.glb'),
        'changed_vertices': int((np.linalg.norm(result - points, axis=1) > 1e-6).sum()),
        'maximum_displacement_m': float(np.linalg.norm(result - points, axis=1).max()),
        'preserved': ['facial features below upper forehead', 'jaw', 'neck seams', 'UV', 'textures', 'topology', 'rig', 'skin weights', 'shared hair geometry'],
        'method': 'Radial canonical scalp projection; smooth transition at upper forehead and lower rear skull',
        'outputs': {name: fit.digest(DIRECTORY / name) for name in ['face_rugged.glb', 'base_rugged.glb']},
        'source_and_license': 'Existing fitted user-provided Tripo head and modular male canonical head; local geometry edit, no new generation',
    }
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
