import numpy as np


def welded_islands(points, faces):
    _, first, welded = np.unique(np.round(points, 6), axis=0, return_index=True, return_inverse=True)
    welded = welded.ravel()
    parent = np.arange(len(first))

    def root(i):
        while parent[i] != i:
            parent[i] = parent[parent[i]]
            i = parent[i]
        return i

    for face in welded[faces]:
        a = root(face[0])
        for b in face[1:]:
            parent[root(b)] = a
    return first, welded, np.array([root(i) for i in welded])


def edge_component_sizes(points, faces, decimals=5):
    """Faces in each face's piece, joined only across shared welded edges."""
    welded = np.unique(np.round(points, decimals), axis=0, return_inverse=True)[1].ravel()[faces]
    across = {}
    for index, ids in enumerate(welded):
        for a, b in zip(ids, np.roll(ids, -1)):
            across.setdefault((min(a, b), max(a, b)), []).append(index)
    piece = np.full(len(faces), -1)
    for start in range(len(faces)):
        if piece[start] >= 0:
            continue
        piece[start], stack = start, [start]
        while stack:
            ids = welded[stack.pop()]
            for a, b in zip(ids, np.roll(ids, -1)):
                for other in across[(min(a, b), max(a, b))]:
                    if piece[other] < 0:
                        piece[other] = start
                        stack.append(other)
    return np.bincount(piece)[piece]
