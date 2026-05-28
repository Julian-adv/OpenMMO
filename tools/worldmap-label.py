# /// script
# dependencies = ["pillow"]
# ///
"""Overlay lore place-name labels on the clean worldgen base map.

Base image (BASE below) is a gitignored worldgen intermediate. Regenerate it
first if missing:
    cargo run --release -p terrain-gen -- preview --seed 42 \\
        --out data/terrain/worldgen_preview
(produces 07_worldmap.png — the grid/label-free relief layer.)

Run:  uv run tools/worldmap-label.py
Place-name data is the shared source data/map_labels.csv (world meters, game
coords) — the SAME table the in-game world map (M key) reads. Edit that CSV to
move/rename places; this script just renders it onto the doc map.

This is a DRAFT auto-placement: only Aldermark is georeferenced from the real
spawn point; continents/cities/seas were assigned to generated terrain features
and are meant to be nudged by hand.
"""
import csv
import glob
import json
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

# Paths are anchored to the repo root (this file lives in tools/), so the
# script runs correctly from any working directory.
ROOT = Path(__file__).resolve().parents[1]
SEED_DIR = ROOT / "data/terrain/worldgen_preview/000000000000002a"
BASE = SEED_DIR / "07_worldmap.png"
META = SEED_DIR / "meta.json"
OUT = ROOT / "doc/map.png"
OUT_W = 2048              # final width (label at base res, then downscale)

if not BASE.exists():
    sys.exit(
        f"base map not found: {BASE}\n"
        "Regenerate it first:\n"
        "  cargo run --release -p terrain-gen -- preview --seed 42 "
        "--out data/terrain/worldgen_preview"
    )

# World/image scale is read from the base's meta so world-coord labels track the
# actual generation config (global_res / world_size_m), not hardcoded defaults.
_cfg = json.loads(META.read_text())["config"] if META.exists() else {}
RES = _cfg.get("global_res", 4096)
MPC = _cfg.get("world_size_m", 32768) / RES   # meters per cell
ORIGIN = RES / 2                              # world origin cell

def world_to_px(x_m, z_m):
    return (x_m / MPC + ORIGIN, z_m / MPC + ORIGIN)

# --- shared label table ---------------------------------------------------
# Source of truth: data/map_labels.csv (id,name,kind,x,z) in world meters.
# kind: continent | capital | city | town | sea
LABELS_CSV = ROOT / "data/map_labels.csv"
with LABELS_CSV.open(newline="") as fh:
    LABELS = [
        {"name": row["name"], "kind": row["kind"],
         "world": (float(row["x"]), float(row["z"]))}
        for row in csv.DictReader(fh)
    ]

# --- style per kind -------------------------------------------------------
def find_font(names):
    for pat in names:
        hits = glob.glob(pat)
        if hits:
            return hits[0]
    return None

SERIF_BOLD = find_font(["/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf"])
SERIF = find_font(["/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf"])

# (font_path, size, fill, stroke_fill, stroke_w, marker_radius, marker_kind)
STYLE = {
    "continent": (SERIF_BOLD, 70, (250, 246, 236), (45, 38, 30), 4, 0, None),
    "capital":   (SERIF_BOLD, 50, (255, 252, 244), (25, 18, 10), 4, 13, "ring2"),
    "city":      (SERIF_BOLD, 38, (255, 252, 244), (25, 18, 10), 3, 9, "ring"),
    "town":      (SERIF_BOLD, 40, (255, 246, 220), (60, 25, 10), 3, 11, "town"),
    "sea":       (SERIF, 46, (126, 200, 240), (18, 38, 66), 3, 0, None),
    "island":    (SERIF, 30, (224, 238, 224), (28, 40, 30), 2, 0, None),
}

img = Image.open(BASE).convert("RGB")
d = ImageDraw.Draw(img)
fonts = {}
def font(path, size):
    key = (path, size)
    if key not in fonts:
        # load_default(size) (Pillow ≥10) keeps the requested size and stays a
        # TrueType font, so anchored text still works if DejaVu isn't found.
        fonts[key] = ImageFont.truetype(path, size) if path else ImageFont.load_default(size)
    return fonts[key]

def draw_marker(cx, cy, r, kind):
    if kind == "ring":
        d.ellipse([cx-r, cy-r, cx+r, cy+r], fill=(245, 210, 80), outline=(25, 18, 8), width=3)
    elif kind == "ring2":
        d.ellipse([cx-r-5, cy-r-5, cx+r+5, cy+r+5], outline=(25, 18, 8), width=3)
        d.ellipse([cx-r, cy-r, cx+r, cy+r], fill=(250, 215, 70), outline=(25, 18, 8), width=3)
    elif kind == "town":   # spawn town: green-rimmed gold dot
        d.ellipse([cx-r, cy-r, cx+r, cy+r], fill=(245, 210, 80), outline=(40, 120, 50), width=4)

for L in LABELS:
    cx, cy = world_to_px(*L["world"]) if "world" in L else L["px"]
    cx, cy = float(cx), float(cy)
    fp, size, fill, stroke, sw, mr, mk = STYLE[L["kind"]]
    f = font(fp, size)
    if mk:
        draw_marker(cx, cy, mr, mk)
    if L["kind"] in ("continent", "sea", "island"):
        # centered label, no marker
        d.text((cx, cy), L["name"], font=f, fill=fill, stroke_fill=stroke,
               stroke_width=sw, anchor="mm")
    else:
        # marker + label to the right, vertically centered
        d.text((cx + mr + 8, cy), L["name"], font=f, fill=fill, stroke_fill=stroke,
               stroke_width=sw, anchor="lm")

if OUT_W and OUT_W != RES:
    img = img.resize((OUT_W, OUT_W), Image.LANCZOS)
img.save(OUT)
print(f"wrote {OUT} ({img.size[0]}x{img.size[1]}) with {len(LABELS)} labels")
