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
