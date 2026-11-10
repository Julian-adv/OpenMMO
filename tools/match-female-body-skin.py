"""Shift the female body's skin colour and roughness to the ranger face's, so the neck seam has nothing to hide.

Writes full-size colour and ORM maps beside the rig; the game build and the face fit both read them.
The source female-base.glb stays as it is.
"""
import hashlib
import io
import json
from pathlib import Path

import numpy as np
from PIL import Image

from lib.glb import read_glb, view_bytes

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUT = ROOT / 'assets/modular_human_female_01/rig_v1/skin_tone_v1'
# Linear-light gains taking the body's neck just under the seam (1.50-1.545 m: linear 0.523, 0.307, 0.203)
# to the fitted head's neck just above it (1.555-1.60 m: linear 0.739, 0.332, 0.177). Measured 2026-11-10.
GAIN = np.array([1.414, 1.082, 0.872])
# Mean roughness 0.383 on the body's neck against 0.636 on the head's.
ROUGHNESS_OFFSET = 0.25
# Skin is saturated; the underwear and unused texels are not and stay as they are.
SKIN_SATURATION = (0.10, 0.22)


def smoothstep(values):
    values = np.clip(values, 0, 1)
    return values * values * (3 - 2 * values)


def to_linear(c):
    return np.where(c <= .04045, c / 12.92, ((c + .055) / 1.055) ** 2.4)


def to_srgb(c):
    return np.where(c <= .0031308, c * 12.92, 1.055 * np.clip(c, 0, None) ** (1 / 2.4) - .055)


def match_color(srgb):
    """srgb: float array 0-1. Returns the matched colour and the skin weight."""
    saturation = (srgb.max(-1) - srgb.min(-1)) / np.maximum(srgb.max(-1), 1e-3)
    weight = smoothstep((saturation - SKIN_SATURATION[0]) / (SKIN_SATURATION[1] - SKIN_SATURATION[0]))[..., None]
    gained = to_linear(srgb) * GAIN
    # Soft knee so painted highlights do not clip flat.
    gained = np.where(gained > .8, .8 + .2 * np.tanh((gained - .8) / .2), gained)
    return srgb * (1 - weight) + to_srgb(gained) * weight, weight


def main():
    doc, raw = read_glb(BASE)
    material = doc['materials'][doc['meshes'][0]['primitives'][0]['material']]['pbrMetallicRoughness']

    def image(slot):
        source = doc['images'][doc['textures'][material[slot]['index']]['source']]
        return np.asarray(Image.open(io.BytesIO(view_bytes(doc, raw, source['bufferView']))).convert('RGB'), dtype=np.float32) / 255

    color, weight = match_color(image('baseColorTexture'))
    orm = image('metallicRoughnessTexture')
    assert orm.shape == color.shape
    orm[..., 1] = np.clip(orm[..., 1] + ROUGHNESS_OFFSET * weight[..., 0], 0, 1)
    OUT.mkdir(parents=True, exist_ok=True)
    for name, pixels in [('body-albedo.png', color), ('body-orm.png', orm)]:
        Image.fromarray(np.round(np.clip(pixels, 0, 1) * 255).astype(np.uint8)).save(OUT / name, optimize=True)
    reference = np.array([[194, 151, 125]], dtype=np.float32) / 255
    record = {
        'date': '2026-11-10', 'source': str(BASE.relative_to(ROOT)), 'source_sha256': hashlib.sha256(BASE.read_bytes()).hexdigest(),
        'linear_gain': GAIN.tolist(), 'roughness_offset': ROUGHNESS_OFFSET, 'skin_saturation_range': list(SKIN_SATURATION),
        'skin_texel_fraction': float((weight > .5).mean()),
        'underwear_skin_colour_srgb': np.round(match_color(reference)[0][0] * 255).astype(int).tolist(),
        'files': {name: hashlib.sha256((OUT / name).read_bytes()).hexdigest() for name in ['body-albedo.png', 'body-orm.png']},
    }
    (OUT / 'record.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record, indent=2))


if __name__ == '__main__':
    main()
