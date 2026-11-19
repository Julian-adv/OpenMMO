"""Sample the fitted cuirass and match the gauntlet's existing UV-mapped steel to it."""
from io import BytesIO
from itertools import islice
from pathlib import Path

import numpy as np
from PIL import Image

from lib.glb import read_glb, view_bytes
from outfits.plate_fitting import plate_roughness

CUIRASS = Path(__file__).resolve().parents[2] / 'assets/items/female_knight_top/tripo_v1/fitted_v4/top_female_knight.glb'


def finish_images(doc, raw):
    material = doc['materials'][0]
    pbr = material['pbrMetallicRoughness']
    for slot in (pbr['baseColorTexture'], pbr['metallicRoughnessTexture'], material['normalTexture']):
        image = doc['images'][doc['textures'][slot['index']]['source']]
        yield image, Image.open(BytesIO(view_bytes(doc, raw, image['bufferView'])))


def cuirass_swatch():
    # Undecorated convex front plate, around bind position (0.074, 1.364, 0.163).
    doc, raw = read_glb(CUIRASS)
    return tuple(np.array(image.crop((501, 162, 629, 290)).convert('RGB'))
                 for _, image in islice(finish_images(doc, raw), 2))


def match_gauntlet_steel(doc, raw):
    swatch_color, swatch_orm = cuirass_swatch()
    target_color = np.median(swatch_color, axis=(0, 1)).astype('f4')
    target_orm = np.median(swatch_orm, axis=(0, 1)).astype('f4')
    images = list(finish_images(doc, raw))
    color, orm, normal = [np.array(image.convert('RGB')) for _, image in images]
    for start in range(0, len(color), 128):
        rows = slice(start, start + 128)
        rgb, pbr, n = (a[rows].astype('f4') for a in (color, orm, normal))
        # Warm brass, dark creases and leather retain their authored finish.
        metal = np.clip((pbr[..., 2] - 110) / 95, 0, 1)
        silver = np.clip((53 - (rgb[..., 0] - rgb[..., 2])) / 22, 0, 1)
        exposed = np.clip((rgb[..., 1] - 42) / 73, 0, 1)
        weight = (metal * silver * exposed)[..., None]
        detail = (rgb[..., 1:2] - 164) * 0.60
        color[rows] = np.clip(rgb + weight * (target_color + detail - rgb), 0, 255).round()
        rough = plate_roughness(pbr[..., 1], pbr[..., 2])
        pbr[..., 1] += weight[..., 0] * (rough - pbr[..., 1])
        pbr[..., 2] += weight[..., 0] * (target_orm[2] - pbr[..., 2])
        orm[rows] = np.minimum(pbr, 255).round()
        n[..., :2] += 0.5 * weight * (127.5 - n[..., :2])
        xy = (n[..., :2] - 127.5) / 127.5
        z = 127.5 * (1 + np.sqrt(np.maximum(0, 1 - np.sum(xy * xy, axis=-1))))
        n[..., 2] += weight[..., 0] * (z - n[..., 2])
        normal[rows] = n.round()

    for (image, _), pixels in zip(images, (color, orm, normal)):
        encoded = BytesIO()
        Image.fromarray(pixels).save(encoded, 'PNG')
        data = encoded.getvalue()
        raw.extend(bytes(-len(raw) % 4))
        doc['bufferViews'][image['bufferView']].update(byteOffset=len(raw), byteLength=len(data))
        image['mimeType'] = 'image/png'
        raw.extend(data)
