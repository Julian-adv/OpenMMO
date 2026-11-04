"""Move the Tripo priest outfit from its generated body pose onto the female base rest pose."""
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

SCALE = 1.8
ELBOW_BLEND = .035
WRIST_BLEND = .025
GLOVE_CLEARANCE = .004


def unit(v):
    return v / np.linalg.norm(v)


def rotation_between(a, b):
    a, b = unit(a), unit(b)
    v, c = np.cross(a, b), np.dot(a, b)
    k = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
    return np.eye(3) + k + k @ k / (1 + c)


def smoothstep(x):
    x = np.clip(x, 0, 1)
    return x * x * (3 - 2 * x)


def rigid(rotation, pivot, target):
    m = np.eye(4)
    m[:3, :3] = rotation
    m[:3, 3] = target - rotation @ pivot
    return m


def apply(m, points):
    return points @ m[:3, :3].T + m[:3, 3]


def icp(points, tree, offset, iterations=30):
    """Point-to-plane rigid fit keeping points `offset` outside the tree surface."""
    m = np.eye(4)
    for _ in range(iterations):
        current = apply(m, points)
        rows, rhs = [], []
        for p in current:
            hit, normal, _, distance = tree.find_nearest(Vector(p))
            if distance > .05:
                continue
            n = np.array(normal)
            rows.append(np.concatenate([np.cross(p, n), n]))
            rhs.append(offset - np.dot(p - np.array(hit), n))
        x = np.linalg.lstsq(np.array(rows), np.array(rhs), rcond=None)[0]
        w, t = x[:3], x[3:]
        angle = np.linalg.norm(w)
        if angle > 1e-12:
            k = np.array([[0, -w[2], w[1]], [w[2], 0, -w[0]], [-w[1], w[0], 0]]) / angle
            r = np.eye(3) + np.sin(angle) * k + (1 - np.cos(angle)) * k @ k
        else:
            r = np.eye(3)
        step = np.eye(4)
        step[:3, :3], step[:3, 3] = r, t
        m = step @ m
        if angle < 1e-5 and np.linalg.norm(t) < 1e-5:
            break
    return m


class ArmPose:
    """Per-side forearm rotation about the elbow and rigid hand fit, blended along the arm."""

    def __init__(self, bones, body_points, body_faces, body_weights, bone_names, arm_points, gloves):
        self.sides = {}
        for side in ('Left', 'Right'):
            shoulder, elbow, wrist = (bones[side + n] for n in ('Arm', 'ForeArm', 'Hand'))
            sleeve = arm_points[side]
            forearm_axis = unit(wrist - elbow)
            s = (sleeve - elbow) @ forearm_axis
            length = np.linalg.norm(wrist - elbow)
            band = sleeve[(s > .25 * length) & (s < .85 * length)]
            slices = [band[np.abs((band - elbow) @ forearm_axis - t) < .015] for t in np.linspace(.3, .8, 6) * length]
            centers = np.array([part.mean(0) for part in slices if len(part) > 10])
            assert len(centers) >= 4, f'{side} forearm centerline has {len(centers)} slices'
            tripo_axis = unit(np.linalg.svd(centers - centers.mean(0))[2][0])
            if np.dot(tripo_axis, forearm_axis) < 0:
                tripo_axis = -tripo_axis
            forearm = rigid(rotation_between(tripo_axis, forearm_axis), elbow, elbow)
            hand_columns = [i for i, n in enumerate(bone_names) if n.startswith(side + 'Hand')]
            hand_faces = body_faces[(body_weights[:, hand_columns].sum(1)[body_faces] > .5).all(1)]
            tree = BVHTree.FromPolygons([Vector(p) for p in body_points], hand_faces.tolist())
            glove = apply(forearm, gloves[side])
            hand = icp(glove, tree, GLOVE_CLEARANCE) @ forearm
            self.sides[side] = dict(shoulder=shoulder, elbow=elbow, wrist=wrist,
                                    tripo_axis=tripo_axis, forearm=forearm, hand=hand,
                                    tripo_wrist=apply(np.linalg.inv(forearm), wrist[None])[0])

    def __call__(self, points, arms, rigid_hand=False):
        """`arms[side]` marks the vertices that follow that arm.

        `rigid_hand` moves them all with the hand fit; gloves need it because blending across the wrist pinched the cuff.
        """
        out = points.copy()
        for side, d in self.sides.items():
            arm = arms[side]
            p = points[arm]
            if rigid_hand:
                out[arm] = apply(d['hand'], p)
                continue
            along_elbow = (p - d['elbow']) @ unit(d['elbow'] - d['shoulder'])
            along_wrist = (p - d['tripo_wrist']) @ d['tripo_axis']
            fore = smoothstep(along_elbow / ELBOW_BLEND + .5)
            hand = smoothstep(along_wrist / WRIST_BLEND + .5)
            a, b = apply(d['forearm'], p), apply(d['hand'], p)
            moved = p + fore[:, None] * (a - p)
            out[arm] = moved + hand[:, None] * (b - moved)
        return out

    def record(self):
        return {side: {'forearm_rotation_deg': float(np.degrees(np.arccos(np.clip((np.trace(d['forearm'][:3, :3]) - 1) / 2, -1, 1)))),
                       'hand_rotation_deg': float(np.degrees(np.arccos(np.clip((np.trace(d['hand'][:3, :3]) - 1) / 2, -1, 1)))),
                       'hand_translation_m': float(np.linalg.norm(apply(d['hand'], d['tripo_wrist'][None])[0] - d['wrist']))}
                for side, d in self.sides.items()}
