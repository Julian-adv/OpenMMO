import sys
from pathlib import Path

import bpy
import numpy as np

source, output, projection = sys.argv[sys.argv.index('--') + 1:]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=source)
head = next(o for o in bpy.data.objects if o.type == 'MESH' and o.name.startswith('face_rugged'))
neck = next(o for o in bpy.data.objects if o.type == 'MESH' and o.name.startswith('face_neck_bridge'))
def base_color_image(obj):
    nodes = obj.data.materials[0].node_tree.nodes
    return next(n.image for n in nodes if n.type == 'TEX_IMAGE' and n.label == 'BASE COLOR')


head_image = base_color_image(head)
body_image = base_color_image(neck)
def sample(image, uv):
    width, height = image.size
    pixels = np.array(image.pixels[:]).reshape(height, width, 4)[..., :3]
    p = np.clip(uv * [width - 1, height - 1], [0, 0], [width - 1.0001, height - 1.0001])
    x, y = np.floor(p).astype(int).reshape(-1, 2).T
    delta = (p - np.floor(p)).reshape(-1, 2)
    a = pixels[y, x] * (1 - delta[:, :1]) + pixels[y, x + 1] * delta[:, :1]
    b = pixels[y + 1, x] * (1 - delta[:, :1]) + pixels[y + 1, x + 1] * delta[:, :1]
    color = (a * (1 - delta[:, 1:]) + b * delta[:, 1:]).reshape(*uv.shape[:2], 3)
    return np.where(color <= .04045, color / 12.92, ((color + .055) / 1.055) ** 2.4)

def blur(color, sigma):
    radius = int(np.ceil(sigma * 3))
    offsets = np.arange(-radius, radius + 1)
    kernel = np.exp(-.5 * (offsets / sigma) ** 2)
    kernel /= kernel.sum()
    for axis in [0, 1]:
        padding = [(radius, radius) if i == axis else (0, 0) for i in range(3)]
        padded = np.pad(color, padding, mode='reflect')
        color = sum(weight * np.take(padded, np.arange(color.shape[axis]) + radius + offset, axis=axis)
            for weight, offset in zip(kernel, offsets))
    return color


maps = np.load(projection)
body_uv, head_uv, donor_uv = [maps[key].copy() for key in ['body_uv', 'head_uv', 'donor_uv']]
body_uv[..., 1] = 1 - body_uv[..., 1]
head_uv[..., 1] = 1 - head_uv[..., 1]
donor_uv[..., 1] = 1 - donor_uv[..., 1]
body_rgb, head_rgb = sample(body_image, body_uv), sample(head_image, head_uv)
donor_rgb = sample(body_image, donor_uv)
blend = maps['blend'][..., None]
original = body_rgb * (1 - blend) + head_rgb * blend
color = blur(body_rgb, 10) * (1 - blend) + blur(head_rgb, 10) * blend
detail = donor_rgb - blur(donor_rgb, 2)
t = (np.indices(blend.shape[:2])[0] / (blend.shape[0] - 1) - .02) / .96
lower = np.clip(t / .12, 0, 1)
upper = np.clip((1 - t) / .12, 0, 1)
fade = (lower * lower * (3 - 2 * lower) * upper * upper * (3 - 2 * upper))[..., None]
linear = np.clip(original * (1 - fade) + (color + detail * .5) * fade, 0, 1)
srgb = np.where(linear <= .0031308, linear * 12.92, 1.055 * linear ** (1 / 2.4) - .055)
rgba = np.concatenate([srgb, np.ones((*srgb.shape[:2], 1))], axis=-1)
image = bpy.data.images.new('Neck transition', srgb.shape[1], srgb.shape[0], alpha=True)
image.pixels.foreach_set(rgba.astype(np.float32).reshape(-1))
image.filepath_raw = output
image.file_format = 'PNG'
image.save()
print('Baked surface-projected neck transition:', str(Path(output)))
