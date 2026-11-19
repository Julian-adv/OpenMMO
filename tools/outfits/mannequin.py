"""Shared steps for cutting an outfit off the blue mannequin it was generated on."""
import io as bytes_io

import numpy as np
from PIL import Image
from scipy import ndimage
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor

# Tripo models face +X; the bodies face +Z.
FACING = np.array([[0, 0, -1], [0, 1, 0], [1, 0, 0]], float)
# Red minus blue below which a texel is mannequin blue that bled into the outfit.
BLEED = -40


def kabsch(a, b):
    """Rotation taking centred points `a` onto `b`, and the summed stretch along its axes."""
    u, s, vt = np.linalg.svd(b.T @ a)
    sign = np.diag([1, 1, np.sign(np.linalg.det(u @ vt))])
    return u @ sign @ vt, (s * np.diag(sign)).sum()


class Source:
    def __init__(self, path):
        self.doc, binary = read_glb(path)
        self.binary = bytearray(binary)
        node = self.doc['nodes'][self.doc['scenes'][0]['nodes'][0]]
        self.primitive = self.doc['meshes'][node['mesh']]['primitives'][0]
        attributes = self.primitive['attributes']
        matrix = np.array(node.get('matrix', np.eye(4).ravel().tolist())).reshape(4, 4).T
        read = lambda name: accessor(self.doc, self.binary, attributes[name]).astype(float)
        self.points = read('POSITION') @ matrix[:3, :3].T + matrix[:3, 3]
        self.normals = read('NORMAL') @ matrix[:3, :3].T
        self.uv = read('TEXCOORD_0')
        self.faces = accessor(self.doc, self.binary, self.primitive['indices']).reshape(-1, 3).astype(int)
        material = self.doc['materials'][self.primitive['material']]
        texture = self.doc['textures'][material['pbrMetallicRoughness']['baseColorTexture']['index']]
        self.image = self.doc['images'][texture['source']]
        view = self.doc['bufferViews'][self.image['bufferView']]
        start = view.get('byteOffset', 0)
        packed = bytes(self.binary[start:start + view['byteLength']])
        self.colour = np.asarray(Image.open(bytes_io.BytesIO(packed)).convert('RGB'))
        # Red minus blue: only the mannequin is clearly negative.
        self.warmth = self.colour[:, :, 0].astype(int) - self.colour[:, :, 2]

    def face_share(self, mask, triangles):
        """Share of each triangle's texels, sampled at seven points, that lie in `mask`."""
        size = mask.shape[0]
        corners = self.uv[triangles]
        centre = corners.mean(1)
        samples = [centre] + [centre + (corners[:, i] - centre) * .6 for i in range(3)] \
            + [(corners[:, i] + corners[:, (i + 1) % 3]) / 2 * .7 + centre * .3 for i in range(3)]

        def texel(coords):
            pixels = np.clip((coords % 1.0) * size, 0, size - 1).astype(int)
            return mask[pixels[:, 1], pixels[:, 0]]
        return np.mean([texel(s) for s in samples], 0)

    def weld(self):
        return np.unique(np.round(self.points, 5), axis=0, return_inverse=True)[1].ravel()

    def components(self, triangles):
        welded = self.weld()[triangles]
        count = welded.max() + 1
        graph = coo_matrix((np.ones(len(welded) * 2),
                            (np.r_[welded[:, 0], welded[:, 1]], np.r_[welded[:, 1], welded[:, 2]])), shape=(count, count))
        return connected_components(graph, directed=False)[1][welded[:, 0]]

    def neighbours(self):
        sides = {}
        for index, triangle in enumerate(self.weld()[self.faces]):
            for i in range(3):
                sides.setdefault(tuple(sorted((triangle[i], triangle[(i + 1) % 3]))), []).append(index)
        around = [[] for _ in self.faces]
        for group in sides.values():
            for index in group:
                around[index] += [other for other in group if other != index]
        return around

    def align(self, mannequin, target, scale, offset, upright=False, rescale=True):
        """Similarity ICP of the mannequin's vertices onto the body; the whole model follows.

        `upright` only turns the model about the vertical: a head alone cannot tell a lean from a shift.
        Without `rescale` the given scale stands.
        """
        tree = cKDTree(target)
        moving = self.points[mannequin]
        rotation = FACING
        for step in range(80):
            placed = scale * moving @ rotation.T + offset
            distance, nearest = tree.query(placed)
            keep = distance < (.06 if step < 30 else .015)
            a, b = moving[keep], target[nearest[keep]]
            a0, b0 = a.mean(0), b.mean(0)
            if upright:
                p, q = a - a0, b - b0
                yaw = np.arctan2((p[:, 2] * q[:, 0] - p[:, 0] * q[:, 2]).sum(), (p[:, 0] * q[:, 0] + p[:, 2] * q[:, 2]).sum())
                rotation = np.array([[np.cos(yaw), 0, np.sin(yaw)], [0, 1, 0], [-np.sin(yaw), 0, np.cos(yaw)]])
                if rescale:
                    scale = ((p @ rotation.T) * q).sum() / (p ** 2).sum()
            else:
                rotation, stretch = kabsch(a - a0, b - b0)
                if rescale:
                    scale = stretch / ((a - a0) ** 2).sum()
            offset = b0 - scale * rotation @ a0
        distance = tree.query(scale * moving @ rotation.T + offset)[0]
        self.points = scale * self.points @ rotation.T + offset
        self.normals = self.normals @ rotation.T
        return {'scale': scale, 'rotation': rotation.tolist(), 'translation': offset.tolist(),
                'mannequin_vertices': len(mannequin), 'mean_distance_m': float(distance.mean()),
                'p95_distance_m': float(np.percentile(distance, 95))}

    def clip(self, triangles, height, normal=(0, 1, 0)):
        """Split triangles crossing the plane `normal · p = height` so each lies on one side of it."""
        normal = np.asarray(normal, float)
        side = self.points[triangles] @ normal > height
        crossing = side.any(1) & ~side.all(1)
        made, added = {}, []

        def cut(a, b):
            key = (min(a, b), max(a, b))
            if key not in made:
                t = (height - self.points[a] @ normal) / ((self.points[b] - self.points[a]) @ normal)
                shading = self.normals[a] + t * (self.normals[b] - self.normals[a])
                added.append((self.points[a] + t * (self.points[b] - self.points[a]), shading / np.linalg.norm(shading),
                              self.uv[a] + t * (self.uv[b] - self.uv[a])))
                made[key] = len(self.points) + len(added) - 1
            return made[key]

        result = [triangles[~crossing]]
        for triangle, above in zip(triangles[crossing], side[crossing]):
            # Rotate so `a` is the corner alone on its side.
            a, b, c = np.roll(triangle, -int(np.flatnonzero(above == (above.sum() == 1))[0]))
            ab, ac = cut(a, b), cut(a, c)
            result.append(np.array([[a, ab, ac], [ab, b, c], [ab, c, ac]]))
        if added:
            self.points = np.vstack([self.points, [x[0] for x in added]])
            self.normals = np.vstack([self.normals, [x[1] for x in added]])
            self.uv = np.vstack([self.uv, [x[2] for x in added]])
        return np.vstack(result)

    def write(self, path, parts):
        """One node per part; mannequin blue that bled into the outfit's texels takes the nearest outfit colour."""
        spread = ndimage.binary_dilation(self.warmth < BLEED, iterations=3)
        nearest = ndimage.distance_transform_edt(spread, return_distances=False, return_indices=True)
        packed = bytes_io.BytesIO()
        Image.fromarray(self.colour[nearest[0], nearest[1]]).save(packed, 'PNG')
        doc, binary = self.doc, self.binary
        binary.extend(b'\0' * (-len(binary) % 4))
        doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': packed.getbuffer().nbytes})
        binary += packed.getvalue()
        self.image.update(bufferView=len(doc['bufferViews']) - 1, mimeType='image/png')
        doc['meshes'], doc['nodes'] = [], []
        for name, triangles in parts.items():
            used, compact = np.unique(triangles, return_inverse=True)
            doc['meshes'].append({'name': name, 'primitives': [{
                'attributes': {'POSITION': add_accessor(doc, binary, self.points[used], 'VEC3'),
                               'NORMAL': add_accessor(doc, binary, self.normals[used], 'VEC3'),
                               'TEXCOORD_0': add_accessor(doc, binary, self.uv[used], 'VEC2')},
                'indices': add_accessor(doc, binary, compact.ravel(), 'SCALAR', 5125),
                'material': self.primitive['material']}]})
            doc['nodes'].append({'name': name, 'mesh': len(doc['meshes']) - 1})
        doc['scenes'][0]['nodes'] = list(range(len(doc['nodes'])))
        doc['buffers'][0]['byteLength'] = len(binary)
        write_glb(path, doc, binary)

    def extent(self, triangles):
        used = self.points[np.unique(triangles)]
        return used.min(0), used.max(0)

    def part_report(self, parts):
        report = {}
        for name, triangles in parts.items():
            low, high = self.extent(triangles)
            report[name] = {'triangles': len(triangles), 'min': low.round(3).tolist(), 'max': high.round(3).tolist()}
        return report
