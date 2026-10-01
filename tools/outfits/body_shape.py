"""Local edits shared by the canonical body and already fitted garments."""
import numpy as np


def smoothstep(value):
    t = np.clip(value, 0, 1)
    return t * t * (3 - 2 * t)


def slim_calves(points, bones):
    out = points.copy()
    y = points[:, 1]
    envelope = smoothstep((y - .27) / .08) * (1 - smoothstep((y - .46) / .08))
    for sign, side in [(1, 'Left'), (-1, 'Right')]:
        knee, ankle = bones[side + 'Leg'], bones[side + 'Foot']
        t = (y - ankle[1]) / (knee[1] - ankle[1])
        axis_z = ankle[2] + t * (knee[2] - ankle[2])
        rear = np.maximum(axis_z - points[:, 2], 0)
        influence = envelope * smoothstep(rear / .045) * (points[:, 0] * sign > 0)
        out[:, 2] += .22 * rear * influence
    return out
