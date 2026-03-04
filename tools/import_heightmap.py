#!/usr/bin/env python3
"""
Import a grayscale heightmap PNG into the game's tile-based terrain format.

Usage:
    uv run --with Pillow --with numpy tools/import_heightmap.py \
        client/public/textures/render.png \
        --min-height 0 --max-height 300 \
        --meters-per-pixel 1.0 \
        --origin-tile 0 0 \
        --terrain-dir server/data/terrain

The source PNG (e.g. from https://tangrams.github.io/heightmapper/) is
expected to be grayscale where 0=min_height and 255=max_height.

Game heightmap format:
    - 65x65 vertices per tile (vertex-based, adjacent tiles overlap by 1)
    - uint16 little-endian, height = value * 0.05 - 500.0
    - File: terrain/height/r{rx:+03}_{rz:+03}/h_{tx:+05}_{tz:+05}.bin
"""

import argparse
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image

TILE_DIM = 64
VERTS_PER_SIDE = TILE_DIM + 1  # 65
HEIGHT_STEP = 0.05
HEIGHT_OFFSET = 500.0
# uint16 range: 0..65535 -> height -500.0 .. +2776.75


def height_to_uint16(h: float) -> int:
    v = int(round((h + HEIGHT_OFFSET) / HEIGHT_STEP))
    return max(0, min(65535, v))


def tile_to_region(tile: int) -> int:
    # Match Rust's div_euclid(16) — Python's // already floors toward negative infinity
    return tile // 16


def heightmap_path(base: Path, tx: int, tz: int) -> Path:
    rx, rz = tile_to_region(tx), tile_to_region(tz)
    region_dir = f"r{rx:+03d}_{rz:+03d}"
    filename = f"h_{tx:+05d}_{tz:+05d}.bin"
    return base / "height" / region_dir / filename


def main():
    parser = argparse.ArgumentParser(
        description="Import a grayscale heightmap PNG into game terrain tiles."
    )
    parser.add_argument("input", help="Path to the source heightmap PNG")
    parser.add_argument(
        "--min-height",
        type=float,
        default=0.0,
        help="Height (m) corresponding to pixel value 0 (default: 0)",
    )
    parser.add_argument(
        "--max-height",
        type=float,
        default=300.0,
        help="Height (m) corresponding to pixel value 255 (default: 300)",
    )
    parser.add_argument(
        "--meters-per-pixel",
        type=float,
        default=1.0,
        help="How many meters each source pixel represents (default: 1.0)",
    )
    parser.add_argument(
        "--origin-tile",
        type=int,
        nargs=2,
        default=[0, 0],
        metavar=("TX", "TZ"),
        help="Tile coordinate of the top-left corner of the import (default: 0 0)",
    )
    parser.add_argument(
        "--terrain-dir",
        type=str,
        default="data/terrain",
        help="Output terrain directory (default: server/data/terrain)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be done without writing files",
    )

    args = parser.parse_args()
    base = Path(args.terrain_dir)
    origin_tx, origin_tz = args.origin_tile

    # Load and convert to grayscale float [0..255]
    img = Image.open(args.input).convert("L")
    src_w, src_h = img.size
    print(f"Source image: {src_w} x {src_h} pixels")

    # If meters_per_pixel != 1.0, resize so 1 pixel = 1 meter
    mpp = args.meters_per_pixel
    if mpp != 1.0:
        new_w = int(round(src_w * mpp))
        new_h = int(round(src_h * mpp))
        print(f"Resampling: {mpp} m/px -> {new_w} x {new_h} meters")
        img = img.resize((new_w, new_h), Image.BILINEAR)
    else:
        new_w, new_h = src_w, src_h

    pixels = np.array(img, dtype=np.float64)  # shape (H, W), values 0..255

    # Map pixel values to heights
    min_h, max_h = args.min_height, args.max_height
    heights = min_h + (pixels / 255.0) * (max_h - min_h)

    # Convert to uint16 terrain values
    terrain = np.clip(
        np.round((heights + HEIGHT_OFFSET) / HEIGHT_STEP), 0, 65535
    ).astype(np.uint16)

    # Calculate tile coverage
    # Image row 0 = north (positive Z), row H-1 = south
    # Image col 0 = west  (negative X), col W-1 = east
    tiles_x = (new_w + TILE_DIM - 1) // TILE_DIM
    tiles_z = (new_h + TILE_DIM - 1) // TILE_DIM

    print(f"Height range: {min_h} .. {max_h} m")
    print(f"Output area: {new_w} x {new_h} meters = {tiles_x} x {tiles_z} tiles")
    print(
        f"Tile range: X [{origin_tx}..{origin_tx + tiles_x - 1}], "
        f"Z [{origin_tz}..{origin_tz + tiles_z - 1}]"
    )

    # Pad terrain array so each tile can extract 65×65 vertices
    padded_w = tiles_x * TILE_DIM + 1  # +1 for the last tile's overlapping edge
    padded_h = tiles_z * TILE_DIM + 1
    if padded_w > terrain.shape[1] or padded_h > terrain.shape[0]:
        padded = np.full((padded_h, padded_w), height_to_uint16(min_h), dtype=np.uint16)
        padded[: terrain.shape[0], : terrain.shape[1]] = terrain
        # Extend edges for boundary tiles
        if terrain.shape[1] < padded_w:
            padded[:terrain.shape[0], terrain.shape[1]:] = terrain[:, -1:]
        if terrain.shape[0] < padded_h:
            padded[terrain.shape[0]:, :] = padded[terrain.shape[0] - 1 : terrain.shape[0], :]
        terrain = padded

    if args.dry_run:
        print("\n[DRY RUN] Would write the following tiles:")

    written = 0
    for tz_off in range(tiles_z):
        for tx_off in range(tiles_x):
            tx = origin_tx + tx_off
            tz = origin_tz + tz_off
            # Extract 65×65 vertex chunk (overlapping with adjacent tiles)
            row_start = tz_off * TILE_DIM
            col_start = tx_off * TILE_DIM
            chunk = terrain[row_start : row_start + VERTS_PER_SIDE, col_start : col_start + VERTS_PER_SIDE]

            # Game format: row-major, z outer loop, x inner loop, little-endian uint16
            data = chunk.astype("<u2").tobytes()
            assert len(data) == VERTS_PER_SIDE * VERTS_PER_SIDE * 2  # 8450 bytes

            fpath = heightmap_path(base, tx, tz)
            if args.dry_run:
                print(f"  {fpath}")
            else:
                fpath.parent.mkdir(parents=True, exist_ok=True)
                fpath.write_bytes(data)
            written += 1

    action = "Would write" if args.dry_run else "Wrote"
    print(f"\n{action} {written} tile files.")


if __name__ == "__main__":
    main()
