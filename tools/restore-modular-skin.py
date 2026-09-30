import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.ndimage import distance_transform_edt, gaussian_filter, map_coordinates

from lib.glb import write_glb

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'assets/modular_human_male_01/parts/fitted/base.glb'
SOURCES = BASE.parents[1] / 'skin_sources'
SURFACE = ROOT / 'doc/images/characters/modular_human_male_01/parts/body_skin_surface.png'


def smoothstep(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def coverage(points):
    y = points[:, 1]
    return smoothstep((y - .61) / .16) * (1 - smoothstep((y - 1.09) / .12))


def rasterize(uv, positions, faces, size):
    for face in faces:
        triangle = uv[face] * size - .5
        lo = np.maximum(np.floor(triangle.min(0)).astype(int), 0)
        hi = np.minimum(np.ceil(triangle.max(0)).astype(int), size - 1)
        if (hi < lo).any():
            continue
        xx, yy = np.meshgrid(np.arange(lo[0], hi[0] + 1), np.arange(lo[1], hi[1] + 1))
        a, b, c = triangle
        matrix = np.column_stack([b - a, c - a])
        if abs(np.linalg.det(matrix)) < 1e-8:
            continue
        vw = (np.stack([xx, yy], axis=-1).reshape(-1, 2) - a) @ np.linalg.inv(matrix).T
        bary = np.column_stack([1 - vw.sum(1), vw])
        inside = (bary >= -1e-5).all(1)
        if inside.any():
            yield yy.ravel()[inside], xx.ravel()[inside], bary[inside] @ positions[face]


def bake_map(kind, surface, primitives):
    pixels = np.asarray(Image.open(SOURCES / f'{kind}.png').convert('RGB')).copy()
    size = len(pixels)
    occupied = np.zeros((size, size), bool)
    changed = occupied.copy()
    for uv, positions, faces, affected in primitives:
        for y, x, points in rasterize(uv, positions, faces, size):
            occupied[y, x] = True
            if not affected:
                continue
            alpha = coverage(points)
            if not (alpha > 0).any():
                continue
            if kind == 'color':
                u = .2 + (points[:, 0] + .245) / .49 * .6
                v = .2 + (1.22 - points[:, 1]) / .62 * .6
                coords = np.array([v * (surface.shape[0] - 1), u * (surface.shape[1] - 1)])
                target = np.column_stack([map_coordinates(surface[:, :, channel], coords, order=1, mode='nearest') for channel in range(3)])
            else:
                target = np.array([128, 128, 255] if kind == 'normal' else [0, 220, 0])
            pixels[y, x] = np.round(pixels[y, x] * (1 - alpha[:, None]) + target * alpha[:, None]).clip(0, 255)
            changed[y[alpha > 0], x[alpha > 0]] = True
    distance, nearest = distance_transform_edt(~changed, return_indices=True)
    pad = ~occupied & (distance <= 8)
    pixels[pad] = pixels[nearest[0][pad], nearest[1][pad]]
    encoded = io.BytesIO()
    Image.fromarray(pixels).save(encoded, format='PNG')
    return encoded.getvalue()


def restore_skin():
    spec = importlib.util.spec_from_file_location('plate', ROOT / 'tools/fit-modular-plate.py')
    plate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plate)
    doc, raw = plate.BASE, plate.BASE_BIN
    fingerprint = hashlib.sha256(SURFACE.read_bytes() + Path(__file__).read_bytes()).hexdigest()
    if doc.get('extras', {}).get('skin_surface_bake') == fingerprint:
        return
    original_uv = json.loads((SOURCES / 'original-uv.json').read_text())
    binary = bytearray(raw)
    skin = next(material for material in doc['materials'] if material['name'] == 'Material_0')
    for material in doc['materials']:
        if material.get('name') in ('covered_skin', 'restored_skin'):
            material.clear()
            material.update(copy.deepcopy(skin), name='restored_skin')
    primitives, smooth = [], []
    for node in doc['nodes']:
        if 'mesh' not in node:
            continue
        region = node.get('extras', {}).get('region')
        for index, primitive in enumerate(doc['meshes'][node['mesh']]['primitives']):
            attrs = primitive['attributes']
            positions = plate.accessor(doc, raw, attrs['POSITION'])
            uv = plate.accessor(doc, raw, attrs['TEXCOORD_0'])
            key = f'{region}:{index}'
            if key in original_uv:
                uv = np.array(original_uv[key])
                if len(uv) != len(positions):
                    raise ValueError(f'Source UV layout changed: {key}')
                attrs['TEXCOORD_0'] = plate.add_accessor(doc, binary, uv, 'VEC2')
            faces = plate.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
            affected = region in ('legs', 'torso')
            primitives.append((uv, positions, faces, affected))
            if affected:
                smooth.append((attrs, positions, uv, faces))
    surface = np.asarray(Image.open(SURFACE).convert('RGB'), dtype=float)
    h, w = surface.shape[:2]
    center = surface[h // 3:h * 2 // 3, w // 3:w * 2 // 3].mean(axis=(0, 1))
    surface = gaussian_filter(surface, sigma=(1.2, 1.2, 0))
    surface = np.clip((surface - center) * .18 + [180, 134, 104], 0, 255)
    maps = [('color', skin['pbrMetallicRoughness']['baseColorTexture']),
            ('normal', skin['normalTexture']),
            ('roughness', skin['pbrMetallicRoughness']['metallicRoughnessTexture'])]
    for kind, info in maps:
        data = bake_map(kind, surface, primitives)
        image = doc['images'][doc['textures'][info['index']]['source']]
        binary.extend(b'\0' * (-len(binary) % 4))
        image.update(bufferView=len(doc['bufferViews']), mimeType='image/png')
        doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(data)})
        binary.extend(data)
    positions = np.vstack([entry[1] for entry in smooth])
    faces, offset = [], 0
    for _, points, _, indices in smooth:
        faces.extend(indices + offset)
        offset += len(points)
    normals = plate.smooth_normals(positions, np.array(faces))
    offset = 0
    for attrs, points, uv, faces in smooth:
        existing = plate.accessor(doc, raw, attrs['NORMAL'])
        blend = coverage(points)[:, None]
        normal = existing * (1 - blend) + normals[offset:offset + len(points)] * blend
        normal /= np.maximum(np.linalg.norm(normal, axis=1, keepdims=True), 1e-8)
        attrs['NORMAL'] = plate.add_accessor(doc, binary, normal, 'VEC3')
        attrs['TANGENT'] = plate.add_accessor(doc, binary, plate.tangents(points, normal, uv, faces), 'VEC4')
        offset += len(points)
    doc.setdefault('extras', {})['skin_surface_bake'] = fingerprint
    write_glb(BASE, doc, plate.compact(doc, binary))
    print('Baked continuous hip/thigh skin with original UVs and blended boundary normals')


if __name__ == '__main__':
    restore_skin()
