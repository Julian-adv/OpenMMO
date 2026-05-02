import { createRng } from '../utils/simplex-noise'
import { smoothstep } from './terrain-constants'
import {
  BYTES_PER_CELL,
  GRASS_DENSITY_LEVELS,
  unpackPrimary,
} from './splat-encoding'

/** Palette slot for grass cells; matches GLOBAL_PALETTE in splatLayerLoader.ts. */
const GRASS_SLOT = 0

/** Grass is placed only where the cell is ≥90% grass (mirrors legacy R>=230 rule). */
export const GRASS_BLEND_MAX = 26

/** Circle scatter parameters shared by initial generation and per-tile regrow. */
const CIRCLE_RADII = [4, 5, 7, 8, 10, 12]
const MAX_RADIUS = 12
const SCATTER_CELL = 64
const CIRCLES_PER_SCATTER = 16
const TALL_GRASS_PROB = 0.3

/**
 * Stamp grass circles onto `densityOut`/`typeOut` for a rectangular region of
 * the world defined by `gridW × gridH` cells starting at world-cell origin
 * `(gridOX, gridOZ)`. Scatter cells are keyed by world coordinates + `seed`,
 * so two overlapping regions with the same seed produce identical output on
 * the overlap. Writes nothing where `grassMask` is 0.
 */
export function scatterGrassCircles(
  gridW: number,
  gridH: number,
  gridOX: number,
  gridOZ: number,
  grassMask: Uint8Array,
  densityOut: Uint8Array,
  typeOut: Uint8Array,
  seed: number
): void {
  const DENSITY_MAX = GRASS_DENSITY_LEVELS - 1

  const scMinX = Math.floor((gridOX - MAX_RADIUS) / SCATTER_CELL)
  const scMaxX = Math.floor((gridOX + gridW - 1 + MAX_RADIUS) / SCATTER_CELL)
  const scMinZ = Math.floor((gridOZ - MAX_RADIUS) / SCATTER_CELL)
  const scMaxZ = Math.floor((gridOZ + gridH - 1 + MAX_RADIUS) / SCATTER_CELL)

  for (let scz = scMinZ; scz <= scMaxZ; scz++) {
    for (let scx = scMinX; scx <= scMaxX; scx++) {
      const cellSeed =
        (seed ^ 0x47524153) +
        Math.imul(scx, 73856093) +
        Math.imul(scz, 19349663)
      const rng = createRng(cellSeed)
      const cellOX = scx * SCATTER_CELL
      const cellOZ = scz * SCATTER_CELL

      for (let c = 0; c < CIRCLES_PER_SCATTER; c++) {
        const wcx = cellOX + rng() * SCATTER_CELL
        const wcz = cellOZ + rng() * SCATTER_CELL
        const radius = CIRCLE_RADII[Math.floor(rng() * CIRCLE_RADII.length)]
        const isTall = rng() < TALL_GRASS_PROB
        const circleDensity = Math.round(DENSITY_MAX * (0.6 + rng() * 0.4))

        const rawMinX = Math.floor(wcx - radius) - gridOX
        const rawMaxX = Math.ceil(wcx + radius) - gridOX
        const rawMinZ = Math.floor(wcz - radius) - gridOZ
        const rawMaxZ = Math.ceil(wcz + radius) - gridOZ
        if (rawMaxX < 0 || rawMinX > gridW - 1) continue
        if (rawMaxZ < 0 || rawMinZ > gridH - 1) continue

        const lcx = Math.floor(wcx) - gridOX
        const lcz = Math.floor(wcz) - gridOZ
        const centerInGrid = lcx >= 0 && lcx < gridW && lcz >= 0 && lcz < gridH
        if (centerInGrid && !grassMask[lcz * gridW + lcx]) continue

        const lMinX = Math.max(0, rawMinX)
        const lMaxX = Math.min(gridW - 1, rawMaxX)
        const lMinZ = Math.max(0, rawMinZ)
        const lMaxZ = Math.min(gridH - 1, rawMaxZ)

        const r2 = radius * radius
        for (let z = lMinZ; z <= lMaxZ; z++) {
          for (let x = lMinX; x <= lMaxX; x++) {
            const wx = gridOX + x
            const wz = gridOZ + z
            const dx = wx - wcx
            const dz = wz - wcz
            if (dx * dx + dz * dz > r2) continue
            const idx = z * gridW + x
            if (!grassMask[idx]) continue

            const ddist = Math.sqrt(dx * dx + dz * dz)
            const falloff = 1.0 - smoothstep(radius * 0.3, radius, ddist)
            const d = Math.round(circleDensity * falloff)
            if (d > densityOut[idx]) {
              densityOut[idx] = d
              typeOut[idx] = isTall ? 1 : 0
            }
          }
        }
      }
    }
  }
}

/** Build a grass-eligibility mask: cells that are ≥90% vegetation-base primary. */
export function buildGrassMask(
  splatField: Uint8Array,
  cellCount: number
): Uint8Array {
  const mask = new Uint8Array(cellCount)
  for (let i = 0; i < cellCount; i++) {
    const pi = i * BYTES_PER_CELL
    if (
      unpackPrimary(splatField[pi]) === GRASS_SLOT &&
      splatField[pi + 2] <= GRASS_BLEND_MAX
    ) {
      mask[i] = 1
    }
  }
  return mask
}
