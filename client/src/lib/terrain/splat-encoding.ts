/**
 * Splatmap V2 encoding — see doc/SPLATMAP_V2.md
 *
 * Cell layout (4 bytes):
 *   byte 0: indices — (primaryIdx << 4) | secondaryIdx, each 0..15
 *   byte 1: reserved — 0 (future: edge jitter seed, material variant, etc.)
 *   byte 2: blend   — 0 = 100% primary, 255 = 100% secondary
 *   byte 3: vegMeta — 0..229 reserved / 230..239 short grass / 240..249 tall grass / 250..255 reserved
 */

import {
  SHORT_GRASS_R_MIN,
  SHORT_GRASS_R_MAX,
  TALL_GRASS_R_MIN,
  TALL_GRASS_R_MAX,
} from '../shaders/grass-material'

export const MAX_PALETTE = 16
export const BYTES_PER_CELL = 4

/** vegMeta ranges are the same numeric values historically used in the R channel. */
export const SHORT_GRASS_MIN = SHORT_GRASS_R_MIN
export const SHORT_GRASS_MAX = SHORT_GRASS_R_MAX
export const TALL_GRASS_MIN = TALL_GRASS_R_MIN
export const TALL_GRASS_MAX = TALL_GRASS_R_MAX
export const GRASS_DENSITY_LEVELS = SHORT_GRASS_R_MAX - SHORT_GRASS_R_MIN + 1

export interface SplatCell {
  primaryIdx: number
  secondaryIdx: number
  blend: number
  vegMeta: number
}

export function packIndices(primaryIdx: number, secondaryIdx: number): number {
  return ((primaryIdx & 0x0f) << 4) | (secondaryIdx & 0x0f)
}

export function unpackPrimary(indices: number): number {
  return (indices >> 4) & 0x0f
}

export function unpackSecondary(indices: number): number {
  return indices & 0x0f
}

export function readCell(buf: Uint8Array, cellIdx: number): SplatCell {
  const pi = cellIdx * BYTES_PER_CELL
  const indices = buf[pi + 0]
  return {
    primaryIdx: unpackPrimary(indices),
    secondaryIdx: unpackSecondary(indices),
    blend: buf[pi + 2],
    vegMeta: buf[pi + 3],
  }
}

export function writeCell(
  buf: Uint8Array,
  cellIdx: number,
  cell: SplatCell
): void {
  const pi = cellIdx * BYTES_PER_CELL
  buf[pi + 0] = packIndices(cell.primaryIdx, cell.secondaryIdx)
  buf[pi + 1] = 0
  buf[pi + 2] = cell.blend
  buf[pi + 3] = cell.vegMeta
}

/** Pack a single-texture cell (both slots = idx, blend = 0). */
export function packSolid(idx: number, vegMeta = 0): SplatCell {
  return { primaryIdx: idx, secondaryIdx: idx, blend: 0, vegMeta }
}

/**
 * Apply a brush stroke of `paintIdx` at strength `s` (0..1) to a cell.
 * Returns the updated cell (does not mutate input). See SPLATMAP_V2.md §7.
 */
export function applyBrush(
  cell: SplatCell,
  paintIdx: number,
  s: number
): SplatCell {
  const clamped = Math.max(0, Math.min(1, s))
  if (clamped <= 0) return cell

  let { primaryIdx, secondaryIdx, blend } = cell
  const { vegMeta } = cell

  if (primaryIdx === paintIdx) {
    blend = Math.round(blend * (1 - clamped))
  } else if (secondaryIdx === paintIdx) {
    blend = Math.round(blend + clamped * (255 - blend))
  } else if (blend < 128) {
    // primary dominates → replace secondary slot
    secondaryIdx = paintIdx
    blend = Math.round(clamped * 255)
  } else {
    // secondary dominates → replace primary slot
    primaryIdx = paintIdx
    blend = Math.round(255 - clamped * 255)
  }

  return { primaryIdx, secondaryIdx, blend, vegMeta }
}

/**
 * Read grass info from vegMeta byte.
 * Returns { density: 0..9, tall: boolean } or { density: 0 } when no grass.
 */
export function readGrass(vegMeta: number): { density: number; tall: boolean } {
  if (vegMeta >= SHORT_GRASS_MIN && vegMeta <= SHORT_GRASS_MAX) {
    return { density: vegMeta - SHORT_GRASS_MIN, tall: false }
  }
  if (vegMeta >= TALL_GRASS_MIN && vegMeta <= TALL_GRASS_MAX) {
    return { density: vegMeta - TALL_GRASS_MIN, tall: true }
  }
  return { density: 0, tall: false }
}

/** Encode grass density (0..9) + type into vegMeta byte. */
export function writeGrass(density: number, tall: boolean): number {
  if (density <= 0) return 0
  const d = Math.min(GRASS_DENSITY_LEVELS - 1, Math.max(0, density))
  return (tall ? TALL_GRASS_MIN : SHORT_GRASS_MIN) + d
}
