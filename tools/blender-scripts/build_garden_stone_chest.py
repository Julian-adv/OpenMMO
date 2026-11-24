"""Cut the lid off the temple's marble chest and rig it to open.

blender -b -P tools/blender-scripts/build_garden_stone_chest.py

Faces above the seam over the key-pattern band, and the ring hanging in front
of it, become the lid, hinged on the box's back edge. A dark, matte well
replaces Tripo's shallow interior, with a stone rim over the cut wall, and the
same dark stone caps the lid's underside.
"""
import json
import os
import sys
from pathlib import Path

import bmesh

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import chest_rig  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/props/garden_stone_chest/tripo/chest_tripo.glb'
WORK = ROOT / 'assets/props/garden_stone_chest/game_v1/garden_stone_chest.blend'
OUT = ROOT / 'client/public/models/objects/garden_stone_chest.glb'
HEIGHT_M = 0.72
TEXTURE = 512
# Source units: the seam under the lid, the band of the box just below it, the
# well's floor a little above the feet, and the height of the recessed panels
# that bound its walls.
SEAM_Z = 0.505
BAND = (0.45, 0.50)
FLOOR_Z = 0.17
PANELS = (0.2, 0.4)
OPEN_DEG = 105
OPEN_SECONDS = 0.8
INNER_MATERIAL = 'stone_chest_inner'
INNER_SHADE = 0.1
INSET = 0.04


def hollow(name, top, bottom, x0, x1, y0, y1, material, patch):
    """An open box sunk into the body: four inner walls and a floor, facing in."""
    corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    verts = [(x, y, top) for x, y in corners] + [(x, y, bottom) for x, y in corners]
    faces = [(i, (i + 1) % 4, (i + 1) % 4 + 4, i + 4) for i in range(4)] + [(4, 5, 6, 7)]
    # Walls unroll along x and down z + y, so no wall smears a single texel.
    uv = chest_rig.tile(patch, x0, x1, bottom + y0, top + y1)
    return chest_rig.mesh(name, verts, faces, material, lambda p: uv((p[0], p[2] + p[1])))


def rim(name, z, outer, inner, material, patch):
    """The cut wall's top: a flat stone frame from the band to the well."""
    ox0, ox1, oy0, oy1 = outer
    ix0, ix1, iy0, iy1 = inner
    verts = [(ox0, oy0, z), (ox1, oy0, z), (ox1, oy1, z), (ox0, oy1, z),
             (ix0, iy0, z), (ix1, iy0, z), (ix1, iy1, z), (ix0, iy1, z)]
    faces = [(i, (i + 1) % 4, (i + 1) % 4 + 4, i + 4) for i in range(4)]
    return chest_rig.mesh(name, verts, faces, material, chest_rig.tile(patch, *outer))


body = chest_rig.import_shell(SOURCE)
band = [v.co for v in body.data.vertices if BAND[0] < v.co.z < BAND[1]]
x0, x1 = min(p.x for p in band), max(p.x for p in band)
front, back = min(p.y for p in band), max(p.y for p in band)
lid = chest_rig.cut_lid(body, SEAM_Z, lambda c: c.z > SEAM_Z or c.y < front - 0.01, 'stone_chest')

inner = body.data.materials[0].copy()
inner.name = INNER_MATERIAL
panels = [v.co for v in body.data.vertices if PANELS[0] < v.co.z < PANELS[1]]
well = (min(p.x for p in panels) + INSET, max(p.x for p in panels) - INSET,
        min(p.y for p in panels) + INSET, max(p.y for p in panels) - INSET)
patch = chest_rig.stone_patch(lid)
rim_patch = chest_rig.stone_patch(body, SEAM_Z - 0.03)

# Tripo's own shallow, band-textured interior gives way to the well.
bm = bmesh.new()
bm.from_mesh(body.data)
wx0, wx1, wy0, wy1 = well
bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.normal.z > -0.5 and f.calc_center_median().z > 0.1 and any(
    wx0 < v.co.x < wx1 and wy0 < v.co.y < wy1 for v in f.verts)], context='FACES')
bm.to_mesh(body.data)
bm.free()

chest_rig.join(body, hollow('stone_chest_well', SEAM_Z, FLOOR_Z, *well, inner, patch))
chest_rig.join(body, rim('stone_chest_rim', SEAM_Z + 0.002, (x0, x1, front, back), well,
                         body.data.materials[0], rim_patch))
chest_rig.join(lid, chest_rig.cap('stone_chest_lid_under', SEAM_Z + 0.005, x0 + INSET, x1 - INSET,
                                  front + INSET, back - INSET, inner, patch, down=True))

low, high, scale = chest_rig.seat_and_hinge(body, lid, HEIGHT_M, back, SEAM_Z)
chest_rig.rig_and_export(body, lid, OPEN_DEG, OPEN_SECONDS, TEXTURE, WORK, OUT)
# Matte, or the walls catch the sky at a glance and read pale.
chest_rig.shade(OUT, INNER_MATERIAL, INNER_SHADE, matte=True)
print('STONE_CHEST', json.dumps({
    'triangles': chest_rig.triangles(body, lid),
    'lid_triangles': chest_rig.triangles(lid),
    'size_m': [round((high[i] - low[i]) * scale, 3) for i in range(3)],
    'box_m': [round((x1 - x0) * scale, 3), round((back - front) * scale, 3)],
    'bytes': OUT.stat().st_size,
}))
