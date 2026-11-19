from io import BytesIO

import numpy as np
from PIL import Image


def similarity(source, target):
    a, b = source - source.mean(0), target - target.mean(0)
    u, singular, vt = np.linalg.svd(a.T @ b)
    sign = np.sign(np.linalg.det(u @ vt))
    rotation = (u * [1, 1, sign]) @ vt
    scale = (singular @ [1, 1, sign]) / np.sum(a * a)
    return rotation, scale, target.mean(0) - source.mean(0) @ rotation * scale


def plate_roughness(roughness, metalness):
    return np.minimum(np.hypot(roughness, 88 * (metalness / 255)), 255)


def plate_roughness_map(data):
    orm_pixels = np.asarray(Image.open(BytesIO(data))).copy()
    roughness = orm_pixels[:, :, 1].astype(np.float32)
    metalness = orm_pixels[:, :, 2].astype(np.float32)
    orm_pixels[:, :, 1] = plate_roughness(roughness, metalness).round().astype(np.uint8)
    encoded = BytesIO()
    Image.fromarray(orm_pixels).save(encoded, 'PNG', compress_level=6)
    return encoded.getvalue()
