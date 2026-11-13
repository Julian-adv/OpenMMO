"""Draws the Aldermark crest: an alder over the river, under the two moons.

Output is a cape print (320x512 RGBA, transparent outside the shield).
Run: .venv/bin/python tools/crests/build_aldermark_crest.py
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw

S = 4
W, H = 320 * S, 512 * S
GOLD = (214, 170, 82, 255)
GOLD_DARK = (150, 110, 44, 255)
FIELD = (24, 62, 46, 255)
SILVER = (226, 230, 232, 255)
RIVER = (120, 168, 196, 255)
OUT = Path(__file__).resolve().parents[2] / "server/crests/aldermark.png"


def shield(cx, top, half, height, shoulder=0.52):
    pts = [(cx - half, top), (cx + half, top), (cx + half, top + height * shoulder)]
    for i in range(1, 25):
        t = i / 24
        pts.append((cx + half * math.cos(t * math.pi / 2) ** 0.8,
                    top + height * shoulder + (height * (1 - shoulder)) * math.sin(t * math.pi / 2)))
    for i in range(23, -1, -1):
        t = i / 24
        pts.append((cx - half * math.cos(t * math.pi / 2) ** 0.8,
                    top + height * shoulder + (height * (1 - shoulder)) * math.sin(t * math.pi / 2)))
    return pts


def main():
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    cx, top, half, height = W // 2, 70 * S, 132 * S, 372 * S
    d.polygon(shield(cx, top, half, height), fill=GOLD_DARK)
    d.polygon(shield(cx, top + 5 * S, half - 5 * S, height - 11 * S), fill=GOLD)
    inner = shield(cx, top + 13 * S, half - 13 * S, height - 29 * S)
    d.polygon(inner, fill=FIELD)

    mask = Image.new("L", (W, H), 0)
    ImageDraw.Draw(mask).polygon(inner, fill=255)
    layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ld = ImageDraw.Draw(layer)

    # River: three wavy bands across the foot of the shield.
    for i, y0 in enumerate((338, 362, 386)):
        pts = []
        for x in range(0, W + 1, S):
            pts.append((x, (y0 + 5 * math.sin(x / (22 * S) + i * 1.3)) * S))
        ld.line(pts, fill=RIVER if i != 1 else SILVER, width=7 * S, joint="curve")

    # Moons: Serin full on the left, the lesser moon a crescent on the right.
    ld.ellipse([(cx - 92 * S), 108 * S, (cx - 56 * S), 144 * S], fill=SILVER)
    ld.ellipse([(cx + 58 * S), 112 * S, (cx + 88 * S), 142 * S], fill=SILVER)
    ld.ellipse([(cx + 68 * S), 106 * S, (cx + 98 * S), 136 * S], fill=FIELD)

    # Alder: a tapering crown of stacked lobes on a straight trunk with roots.
    trunk_top, ground = 190 * S, 330 * S
    ld.polygon([(cx - 6 * S, trunk_top), (cx + 6 * S, trunk_top),
                (cx + 11 * S, ground), (cx - 11 * S, ground)], fill=GOLD)
    for side in (-1, 1):
        ld.polygon([(cx + side * 6 * S, ground - 16 * S), (cx + side * 44 * S, ground + 6 * S),
                    (cx + side * 40 * S, ground + 12 * S), (cx, ground + 2 * S)], fill=GOLD)
    lobes = [(0, 138, 24), (-22, 160, 24), (22, 160, 24), (0, 172, 28),
             (-40, 190, 26), (40, 190, 26), (-14, 198, 30), (14, 198, 30),
             (-58, 224, 26), (58, 224, 26), (-30, 230, 32), (30, 230, 32), (0, 236, 34),
             (-66, 258, 22), (66, 258, 22), (-40, 264, 28), (40, 264, 28), (0, 268, 30)]
    for dx, y, r in lobes:
        ld.ellipse([cx + (dx - r) * S, (y - r) * S, cx + (dx + r) * S, (y + r) * S], fill=GOLD)
    # Shadowed undersides give the crown its tiers.
    for dx, y, r in ((-30, 206, 16), (30, 206, 16), (0, 184, 14), (-48, 242, 14), (48, 242, 14), (0, 250, 16)):
        ld.arc([cx + (dx - r) * S, (y - r) * S, cx + (dx + r) * S, (y + r) * S], 20, 160,
               fill=GOLD_DARK, width=3 * S)

    img.paste(layer, (0, 0), Image.composite(layer.getchannel("A"), Image.new("L", (W, H), 0), mask))
    out = img.resize((320, 512), Image.LANCZOS)
    out.save(OUT)

if __name__ == "__main__":
    main()
