"""Cut the lid off the kneeling excavator's strongbox and rig it to open.

blender -b -P tools/blender-scripts/build_garden_relic_chest.py

The Tripo model is one shell. Faces above the lid seam inside the box's
footprint become the lid, hinged on the box's back edge. It opens only to 80°,
short of the bowed head and arms. Caps of the same stone, shaded darker, close
the box mouth and the lid's underside, which the generator left hollow.
"""
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import chest_rig  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/props/garden_relic_chest/tripo/relic_tripo.glb'
WORK = ROOT / 'assets/props/garden_relic_chest/game_v1/garden_relic_chest.blend'
OUT = ROOT / 'client/public/models/objects/garden_relic_chest.glb'
HEIGHT_M = 1.15
TEXTURE = 512
# Source units, before scaling: the groove under the lid, a height above the
# lid and below the bowed head, the box's back edge measured from the front
# face, and its half width inside the gripping hands.
SEAM_Z = 0.232
LID_TOP_Z = 0.38
BOX_DEPTH = 0.30
BOX_HALF_WIDTH = 0.33
OPEN_DEG = 80
OPEN_SECONDS = 0.8
INNER_MATERIAL = 'relic_inner'
INNER_SHADE = 0.35

body = chest_rig.import_shell(SOURCE)
front = min(v.co.y for v in body.data.vertices)
back = front + BOX_DEPTH
lid = chest_rig.cut_lid(body, SEAM_Z, lambda c: SEAM_Z < c.z < LID_TOP_Z and c.y < back and abs(c.x) < BOX_HALF_WIDTH,
                        'relic')

inset = 0.02
inner = body.data.materials[0].copy()
inner.name = INNER_MATERIAL
rect = (-BOX_HALF_WIDTH + inset, BOX_HALF_WIDTH - inset, front + inset, back - inset)
patch = chest_rig.stone_patch(lid)
chest_rig.join(body, chest_rig.cap('relic_mouth', SEAM_Z - 0.03, *rect, inner, patch))
chest_rig.join(lid, chest_rig.cap('relic_lid_under', SEAM_Z + 0.005, *rect, inner, patch, down=True))

low, high, scale = chest_rig.seat_and_hinge(body, lid, HEIGHT_M, back, SEAM_Z)
chest_rig.rig_and_export(body, lid, OPEN_DEG, OPEN_SECONDS, TEXTURE, WORK, OUT)
chest_rig.shade(OUT, INNER_MATERIAL, INNER_SHADE)
print(json.dumps({
    'triangles': chest_rig.triangles(body, lid),
    'lid_triangles': chest_rig.triangles(lid),
    'size_m': [round((high[i] - low[i]) * scale, 3) for i in range(3)],
    'bytes': OUT.stat().st_size,
}))
