"""Shared geometry and skinning helpers for outfit fitting."""
from collections import Counter

import numpy as np


def compact_weights(dense):
    joints = np.argsort(dense, axis=1)[:, -4:][:, ::-1]
    weights = np.take_along_axis(dense, joints, axis=1)
    return joints, weights / weights.sum(axis=1, keepdims=True)


def edge_loops(faces):
    counts = Counter(tuple(sorted(edge)) for face in faces for edge in zip(face, np.roll(face, -1)))
    neighbors = {}
    for (a, b), count in counts.items():
        if count == 1:
            neighbors.setdefault(a, []).append(b)
            neighbors.setdefault(b, []).append(a)
    assert all(len(adjacent) == 2 for adjacent in neighbors.values())
    loops = []
    while neighbors:
        start = next(iter(neighbors))
        loop, previous, current = [], None, start
        while True:
            loop.append(current)
            nxt = next(vertex for vertex in neighbors[current] if vertex != previous)
            previous, current = current, nxt
            if current == start:
                break
        for index in loop:
            del neighbors[index]
        loops.append(loop)
    return loops


def clip_garment(points, faces, uv, distance):
    points, uv = np.asarray(points), np.asarray(uv)
    output, coords, triangles = [], [], []
    for face in faces:
        face_uv = uv[face].copy()
        if np.ptp(face_uv[:, 0]) > .5:
            face_uv[face_uv[:, 0] < .5, 0] += 1
        polygon = []
        for corner, (a, b) in enumerate(zip(face, np.roll(face, -1))):
            ta, tb = face_uv[corner], face_uv[(corner + 1) % 3]
            if distance[a] <= 0:
                polygon.append((points[a], ta))
            if distance[a] * distance[b] < 0:
                t = distance[a] / (distance[a] - distance[b])
                polygon.append((points[a] + t * (points[b] - points[a]), ta + t * (tb - ta)))
        start = len(output)
        output.extend(p[0] for p in polygon)
        coords.extend(p[1] for p in polygon)
        triangles.extend([[start, start + i, start + i + 1] for i in range(1, len(polygon) - 1)])
    return np.array(output), np.array(triangles), np.array(coords)


def wrist_section(triangles, origin, axis, basis, radial, mid=None, outermost=True):
    distance = (triangles - origin) @ axis
    crossing = (distance.min(axis=1) < 0) & (distance.max(axis=1) > 0)
    segments = []
    for tri, d in zip(triangles[crossing], distance[crossing]):
        hits = []
        for a, b in [(0, 1), (1, 2), (2, 0)]:
            if d[a] * d[b] < 0:
                t = d[a] / (d[a] - d[b])
                hits.append((tri[a] + t * (tri[b] - tri[a]) - origin) @ basis.T)
        if len(hits) == 2:
            segments.append(hits)
    segments = np.array(segments)
    if mid is None:
        mid = (segments.reshape(-1, 2).min(axis=0) + segments.reshape(-1, 2).max(axis=0)) / 2
    start, edge = segments[:, 0] - mid, segments[:, 1] - segments[:, 0]
    cross = lambda a, b: a[..., 0] * b[..., 1] - a[..., 1] * b[..., 0]
    denominator = cross(radial[:, None], edge)
    along = np.divide(cross(start, edge)[None, :], denominator, out=np.full_like(denominator, -np.inf), where=abs(denominator) > 1e-10)
    between = np.divide(cross(start, radial[:, None]), denominator, out=np.full_like(denominator, -np.inf), where=abs(denominator) > 1e-10)
    valid = (between >= 0) & (between <= 1) & (along > 0)
    radius = np.where(valid, along, -np.inf).max(axis=1) if outermost else np.where(valid, along, np.inf).min(axis=1)
    assert np.isfinite(radius).all(), f'Open body section at {origin.tolist()}'
    return mid, radius
