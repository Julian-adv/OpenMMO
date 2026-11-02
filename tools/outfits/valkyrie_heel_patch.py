import numpy as np


def patches(points, faces):
    eye = np.array([-2.4, 2.4, -.45])
    target = np.array([0., 0., .08])
    forward = target - eye
    forward /= np.linalg.norm(forward)
    right = np.cross(forward, [0, 0, 1])
    right /= np.linalg.norm(right)
    up = np.cross(right, forward)
    world = np.column_stack([points[:, 0], -points[:, 2], points[:, 1]]) - target
    screen = np.column_stack([world @ right, world @ up])
    regions = [
        ([2705, 2711, 2713], [(2712, 2), (2711, 2), (2708, 0)]),
    ]
    result = []
    for face_ids, anchors in regions:
        boundary = np.array([screen[faces[face, vertex]] for face, vertex in anchors])
        center = boundary.mean(axis=0)
        boundary = center + (boundary - center) * 1.08
        first, second = boundary[1] - boundary[0], boundary[2] - boundary[0]
        signed_area = first[0] * second[1] - first[1] * second[0]
        orientation = np.sign(signed_area)
        assert orientation != 0
        for face in face_ids:
            coords = screen[faces[face]]
            current = list(np.eye(3))
            outside = []
            for edge in range(3):
                a, b = boundary[edge], boundary[(edge + 1) % 3]
                direction = b - a

                def side(bary):
                    p = bary @ coords - a
                    return orientation * (direction[0] * p[1] - direction[1] * p[0])

                def clip(polygon, keep_inside):
                    output = []
                    for p, q in zip(polygon, polygon[1:] + polygon[:1]):
                        dp, dq = side(p), side(q)
                        ip, iq = (dp >= -1e-12, dq >= -1e-12) if keep_inside else (dp < -1e-12, dq < -1e-12)
                        if ip:
                            output.append(p)
                        if ip != iq:
                            output.append(p + (q - p) * (dp / (dp - dq)))
                    return output

                if not current:
                    break
                rejected = clip(current, False)
                if len(rejected) >= 3:
                    outside.append(rejected)
                current = clip(current, True)
            result.append((face, outside, current))
    return result
