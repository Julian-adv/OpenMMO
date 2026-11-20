"""Smooth, edged sheet metal and leather for the reference-based knight tassets."""

from io import BytesIO

import numpy as np
from PIL import Image
from scipy.ndimage import gaussian_filter

from lib.glb_accessors import add_accessor, add_view
from outfits.knight_steel import cuirass_swatch

STEEL, GUNMETAL, EDGE, BRASS, LEATHER, LINING = range(6)
KINDS = dict(POSITION="VEC3", NORMAL="VEC3", TEXCOORD_0="VEC2", JOINTS_0="VEC4", WEIGHTS_0="VEC4")


def unit(a):
    return a / np.maximum(np.linalg.norm(a, axis=-1, keepdims=True), 1e-12)


def rigid(joint):
    return lambda u, v: ([joint, 0, 0, 0], [1, 0, 0, 0])


def add_material(doc, data, name, image_names, extra_finishes=()):
    doc["images"], doc["textures"] = [], []
    for title, png in zip(image_names, atlas(extra_finishes)):
        doc["images"].append(dict(name=title, mimeType="image/png", bufferView=add_view(doc, data, png)))
        doc["textures"].append(dict(source=len(doc["textures"])))
    doc["materials"] = [
        dict(
            name=name,
            doubleSided=True,
            pbrMetallicRoughness=dict(baseColorTexture=dict(index=0), metallicRoughnessTexture=dict(index=1)),
        )
    ]
    doc["buffers"] = [dict(byteLength=len(data))]


def atlas(extra_finishes=()):
    rng = np.random.default_rng(44)
    base, orm = [], []
    steel_color, steel_orm = cuirass_swatch()

    def strip(tile):
        mirrored = np.concatenate([tile, tile[:, ::-1]], axis=1)
        return np.tile(mirrored, (1, 4, 1))

    finishes = [
        None, None, None,
        ([161, 135, 93], 108, 238),
        ([72, 45, 32], 161, 8),
        ([48, 37, 32], 205, 0),
        *extra_finishes,
    ]
    for slot, finish in enumerate(finishes):
        grain = rng.normal(size=(128, 1024))
        if finish is None:
            shade = {GUNMETAL: 0.97, EDGE: 0.94}.get(slot, 1.0)
            base.append(strip(steel_color) * shade)
            orm.append(strip(steel_orm))
            continue
        color, rough, metal, *grain_scale = finish
        brushed = gaussian_filter(grain, (0.65, 7))
        broad = gaussian_filter(grain, (8, 20))
        noise = 1.6 * grain + 1.3 * brushed / brushed.std() + 1.8 * broad / broad.std()
        y, x = np.mgrid[0:128, 0:1024]
        edge = np.exp(-np.minimum(x, 1023 - x) / 13) + np.exp(-np.minimum(y, 127 - y) / 4)
        if metal > 220:
            wear = 2.7 * edge - 6 * np.exp(-y / 11)
            noise += wear + 1.8 * gaussian_filter(grain, (0.4, 18)) / brushed.std()
        else:
            noise += 4 * np.tanh(grain * 1.3) - 7 * edge
        if grain_scale:
            noise *= grain_scale[0]
        base.append(np.clip(color + noise[:, :, None], 0, 255))
        orm.append(
            np.stack(
                [np.full_like(grain, 255), np.clip(rough + noise * 2, 0, 255), np.full_like(grain, metal)],
                axis=-1,
            )
        )
    result = []
    for strips in [base, orm]:
        buf = BytesIO()
        Image.fromarray(np.concatenate(strips).round().astype("u1")).save(buf, "PNG")
        result.append(buf.getvalue())
    return result


class SurfaceMesh:
    def __init__(self, material_count=6):
        self.material_count = material_count
        self.attrs = {key: [] for key in KINDS}
        self.faces = []
        self.vertices = 0
        self.triangles = 0

    def primitive(self, doc, data, values=None):
        values = values or {k: np.concatenate(v) for k, v in self.attrs.items()}
        attrs = {
            k: add_accessor(doc, data, v, KINDS[k], 5123 if k == "JOINTS_0" else 5126) for k, v in values.items()
        }
        assert self.vertices < 65536
        indices = add_accessor(doc, data, np.concatenate(self.faces).ravel(), "SCALAR", 5123)
        return dict(attributes=attrs, indices=indices, material=0)

    def append(self, points, normals, uv, faces, material, joints, weights):
        points = np.asarray(points).reshape(-1, 3)
        uv = np.asarray(uv).reshape(-1, 2) * [0.97, 0.90] + [0.015, 0.05]
        uv[:, 1] = (material + uv[:, 1]) / self.material_count
        data = dict(
            POSITION=points,
            NORMAL=unit(np.asarray(normals).reshape(-1, 3)),
            TEXCOORD_0=uv,
            JOINTS_0=np.broadcast_to(joints, (len(points), 4)),
            WEIGHTS_0=np.broadcast_to(weights, (len(points), 4)),
        )
        for key, values in data.items():
            self.attrs[key].append(values)
        self.faces.append(np.asarray(faces).reshape(-1, 3) + self.vertices)
        self.vertices += len(points)
        self.triangles += len(faces)

    def sheet(self, fn, u, v, outward, material, skin, thickness):
        U, V = np.meshgrid(u, v)

        def frame(a, b):
            pos = fn(a, b)
            h = 1e-4
            du = fn(a + h, b) - fn(a - h, b)
            dv = fn(a, b + h) - fn(a, b - h)
            n = unit(np.cross(du, dv))
            return pos, n

        pts, normals = frame(U, V)
        mid = (len(v) // 2, len(u) // 2)
        orientation = 1 if normals[mid] @ outward > 0 else -1
        normals *= orientation
        ids = np.arange(U.size).reshape(U.shape)
        a, b, c, d = ids[:-1, :-1], ids[:-1, 1:], ids[1:, 1:], ids[1:, :-1]
        faces = np.stack([a, b, c, a, c, d], -1).reshape(-1, 3)
        if orientation < 0:
            faces = faces[:, ::-1]
        uv = np.stack([U, V], -1).reshape(-1, 2)
        joint, weight = skin(U, V)
        self.append(pts, normals, uv, faces, material, joint, weight)
        def anchor(a, b):
            point, normal = frame(np.asarray(a), np.asarray(b))
            j, w = skin(np.asarray(a), np.asarray(b))
            return point, normal * orientation, j, w

        if thickness == 0:
            return anchor
        # A narrow bevel separates the overlapping lames.
        border = np.r_[ids[0], ids[1:, -1], ids[-1, -2::-1], ids[-2:0:-1, 0]]
        if orientation < 0:
            border = border[::-1]
        p, n = pts.reshape(-1, 3)[border], normals.reshape(-1, 3)[border]
        tangent = unit(np.roll(p, -1, axis=0) - np.roll(p, 1, axis=0))
        out = unit(np.cross(tangent, n))
        boundary = len(p)
        j = np.broadcast_to(joint, (U.size, 4))[border]
        w = np.broadcast_to(weight, (U.size, 4))[border]
        i = np.arange(boundary)
        nxt = (i + 1) % boundary
        rim = np.stack([i, i + boundary, nxt, nxt, i + boundary, nxt + boundary], -1).reshape(-1, 3)
        self.append(
            np.vstack([p, p + thickness * (0.16 * out - n)]),
            np.vstack([unit(0.8 * out + 0.6 * n), out]),
            np.tile(uv[border], (2, 1)),
            rim,
            EDGE if material < BRASS else material,
            np.tile(j, (2, 1)),
            np.tile(w, (2, 1)),
        )

        return anchor

    def stud(self, anchor, u, v, radius, material=BRASS):
        p, n, j, w = anchor(u, v)
        t = unit(np.cross(n, [0, 1, 0] if abs(n[1]) < 0.9 else [1, 0, 0]))
        b = np.cross(n, t)
        theta = np.arange(9) * (2 * np.pi / 9)
        radial = np.outer(np.cos(theta), t) + np.outer(np.sin(theta), b)
        ns = np.vstack([unit(radial + n * 0.25), unit(0.5 * radial + n), n])
        pts = np.vstack([p + radius * radial, p + radius * (0.68 * radial + 0.41 * n), p + radius * 0.54 * n])
        i = np.arange(9)
        nxt = (i + 1) % 9
        faces = np.stack([i, nxt, i + 9, nxt, nxt + 9, i + 9, i + 9, nxt + 9, np.full(9, 18)], -1).reshape(
            -1, 3
        )
        self.append(pts, ns, np.full((19, 2), 0.5), faces, material, j, w)
