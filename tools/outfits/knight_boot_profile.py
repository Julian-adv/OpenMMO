"""Equipped leg sections and thigh cuff of the female knight boots, in metres."""

import numpy as np
from scipy.interpolate import PchipInterpolator


def xyz(x, y, z):
    return np.stack(np.broadcast_arrays(x, y, z), axis=-1)


# Height, lateral centre, depth centre and the two radii of the shaped last.
_stations = np.array(
    [
        [0.085, 0.247, -0.048, 0.048, 0.054],
        [0.13, 0.242, -0.054, 0.040, 0.045],
        [0.18, 0.235, -0.060, 0.031, 0.039],
        [0.24, 0.225, -0.065, 0.033, 0.044],
        [0.32, 0.209, -0.072, 0.043, 0.054],
        [0.405, 0.192, -0.077, 0.056, 0.063],
        [0.45, 0.185, -0.075, 0.057, 0.063],
        [0.50, 0.175, -0.064, 0.053, 0.058],
        [0.545, 0.162, -0.050, 0.051, 0.055],
        [0.58, 0.151, -0.037, 0.051, 0.054],
        [0.635, 0.141, -0.027, 0.052, 0.058],
        [0.69, 0.133, -0.021, 0.060, 0.066],
        [0.76, 0.123, -0.018, 0.071, 0.078],
        [0.82, 0.114, -0.014, 0.083, 0.091],
        [0.89, 0.100, -0.011, 0.097, 0.099],
    ]
)
leg_section = PchipInterpolator(_stations[:, 0], _stations[:, 1:], axis=0)


def leg(side, a, y, extra=0):
    cx, cz, rx, rz = np.moveaxis(leg_section(y), -1, 0)
    return xyz(side * (cx + (rx + extra) * np.sin(a)), y, cz + (rz + extra) * np.cos(a))


def cuff(a):
    return 0.824 + 0.037 * np.sin(a) + 0.008 * np.cos(a) - 0.008 * np.cos(2 * a)


def rim_offset(a, v):
    return 0.0077 + 0.0010 * (1 - v) + 0.0033 * np.maximum(0, np.cos(a)) ** 10
