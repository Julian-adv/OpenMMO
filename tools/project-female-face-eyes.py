"""Unlit front views of a fitted face's eyes (--face), the inputs of its iris retouch.

Writes front-<side>.png, visible-<side>.png (the eyeball texels seen from the front) and
input-<side>.png (the front view padded and enlarged for the image tool) to iris_cleanup_v1/.
"""
import argparse
import importlib.util
from pathlib import Path

import numpy as np
import trimesh
from PIL import Image

from lib.glb import read_glb

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-female-ranger-face.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
PAD, INPUT_SIZE = 13, (1536, 1022)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--face', choices=list(fit.FACES), required=True)
    parser.add_argument('--out', help='Directory to write to instead of iris_cleanup_v1')
    args = parser.parse_args()
    fit.configure(args.face)
    out = Path(args.out) if args.out else fit.IRIS
    out.mkdir(parents=True, exist_ok=True)
    doc, raw = read_glb(fit.SOURCE)
    attrs, faces = fit.custom.geometry(doc, raw, doc['meshes'][0]['primitives'][0])
    points, uvs = attrs['POSITION'].astype(np.float64), attrs['TEXCOORD_0'].astype(np.float64)
    color = fit.material_image(doc, raw, doc['materials'][0]['pbrMetallicRoughness']['baseColorTexture'])
    _, lashes, balls = fit.eye_parts(points, faces, 200)
    keep = np.ones(len(faces), dtype=bool)
    keep[np.concatenate(list(lashes.values()))] = False
    eye = np.zeros(len(faces), dtype=bool)
    eye[np.concatenate(list(balls.values()))] = True
    mesh = trimesh.Trimesh(points, faces[keep], process=False)
    width, height = fit.IRIS_SIZE
    col, row = np.meshgrid(np.arange(width), np.arange(height))
    for side in ('L', 'R'):
        z = (col.ravel() - width / 2) / fit.IRIS_SCALE + fit.IRIS_Z[side]
        y = fit.IRIS_Y + (height / 2 - row.ravel()) / fit.IRIS_SCALE
        origins = np.column_stack([np.ones_like(z), y, z])
        hits, rays, triangles = mesh.ray.intersects_location(origins, np.tile([-1., 0, 0], (len(z), 1)), multiple_hits=False)
        corners = faces[keep][triangles]
        bary = trimesh.triangles.points_to_barycentric(points[corners], hits)
        sampled = fit.custom.texture_sample(color, np.einsum('ij,ijk->ik', bary, uvs[corners]))
        front = np.zeros((width * height, 3), dtype=np.float32)
        front[rays] = sampled
        visible = np.zeros(width * height, dtype=np.uint8)
        visible[rays] = eye[keep][triangles] * 255
        image = Image.fromarray(np.round(front.reshape(height, width, 3) * 255).astype(np.uint8))
        image.save(out / f'front-{side}.png')
        Image.fromarray(visible.reshape(height, width)).save(out / f'visible-{side}.png')
        pixels = np.asarray(image)
        padded = np.concatenate([np.repeat(pixels[:1], PAD, 0), pixels, np.repeat(pixels[-1:], PAD, 0)])
        Image.fromarray(padded).resize(INPUT_SIZE, Image.LANCZOS).save(out / f'input-{side}.png')


if __name__ == '__main__':
    main()
