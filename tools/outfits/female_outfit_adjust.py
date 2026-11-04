"""Bake fit adjustments saved from the female preview (client/src/previews/female-part-fit.ts)."""
import json
from datetime import datetime, timezone

import numpy as np

# Blender z-up to glTF y-up, matching the preview's coordinates.
TO_GLTF = np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0]], dtype=float)


def rotation(axis, degrees):
    a = np.radians(degrees)
    c, s = np.cos(a), np.sin(a)
    i, j = [(1, 2), (2, 0), (0, 1)][axis]
    m = np.eye(3)
    m[i, i], m[i, j], m[j, i], m[j, j] = c, -s, s, c
    return m


def matrix(fit, center, side):
    scale = fit['scale'] / 100
    r = rotation(0, fit['rotate-x']) @ rotation(1, fit['rotate-y'] * side) @ rotation(2, fit['rotate-z'] * side)
    linear = r @ np.diag([scale * fit[f'scale-{k}'] / 100 for k in 'xyz'])
    move = np.array([fit['move-x'] / 100 * side, fit['move-y'] / 100, fit['move-z'] / 100])
    return linear, center + move - linear @ center


def apply(points, fit, paired):
    """`fit` is one fit, or {'left': ..., 'right': ...} for a paired part (left is +x)."""
    p = points @ TO_GLTF.T
    sides = np.where((p[:, 0] >= 0) | (not paired), 1, -1)
    out = p.copy()
    for side in np.unique(sides):
        selected = sides == side
        center = (p[selected].min(0) + p[selected].max(0)) / 2
        side_fit = fit[{1: 'left', -1: 'right'}[side]] if 'left' in fit else fit
        linear, offset = matrix(side_fit, center, side)
        out[selected] = p[selected] @ linear.T + offset
    return out @ TO_GLTF


def bake(path, part, points, paired, keep_pending=False):
    """Apply every baked adjustment in order, then the pending one, which moves to the baked list.

    With `keep_pending` the pending adjustment is left for the preview to keep applying on top.
    """
    if not path.exists():
        return points, []
    saved = json.loads(path.read_text())
    entry = saved['parts'].get(part, {})
    history = entry.get('baked', [])
    pending = None if keep_pending else entry.pop('pending', None)
    if pending:
        history.append({**pending, 'baked_at': datetime.now(timezone.utc).isoformat()})
        entry['baked'] = history
        saved['parts'][part] = entry
        path.write_text(json.dumps(saved, indent=2) + '\n')
    for fit in history:
        points = apply(points, fit, paired)
    return points, history


# Sleeve-end offsets move a sleeve's end across the upper arm, linearly from this far below the shoulder
# joint; sleeve vertices lie within SLEEVE_RADIUS of the upper-arm axis. Same as sleeveEndDeformer in female-part-fit.ts.
SLEEVE_START = .09
SLEEVE_RADIUS = .07
SLEEVE_END_INSET = .015


def sleeve_end(points, joints, ends):
    """`joints[side]` is the (shoulder, elbow) bind position in glTF space; `ends` holds cm offsets per side."""
    p = points @ TO_GLTF.T
    for side, key in ((1, 'left'), (-1, 'right')):
        shoulder, elbow = (np.asarray(v, dtype=float) for v in joints[side])
        axis = (elbow - shoulder) / np.linalg.norm(elbow - shoulder)
        forward = np.array([0., 0., 1.]) - axis[2] * axis
        forward /= np.linalg.norm(forward)
        out = np.cross(forward, axis) * side
        d = p - shoulder
        s = d @ axis
        r = np.linalg.norm(d - s[:, None] * axis, axis=1)
        sleeve = (p[:, 0] * side > shoulder[0] * side) & (r < SLEEVE_RADIUS) & (s > SLEEVE_START)
        if not sleeve.any():
            continue
        end = s[sleeve].max() - SLEEVE_END_INSET
        t = (s[sleeve] - SLEEVE_START) / (end - SLEEVE_START)
        offset = ends[key]
        p[sleeve] += t[:, None] * (offset['forward'] / 100 * forward + offset['out'] / 100 * out)
    return p @ TO_GLTF


def sleeve_end_total(history):
    """Sum the sleeve-end offsets saved with each baked or pending fit."""
    total = {key: {'forward': 0., 'out': 0.} for key in ('left', 'right')}
    for fit in history:
        for key, offset in fit.get('sleeve-end', {}).items():
            for axis in ('forward', 'out'):
                total[key][axis] += offset[axis]
    return total
