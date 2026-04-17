import { createRng } from '../utils/simplex-noise'
import {
  REGION_CELLS,
  SNOW_FULL_HEIGHT,
  SNOW_START_HEIGHT,
  smoothstep,
  type TerrainGenConfig,
} from './terrain-constants'
import {
  BYTES_PER_CELL,
  GRASS_DENSITY_LEVELS,
  VEGMETA_OFFSET,
  packIndices,
  unpackPrimary,
  writeGrass,
} from './splat-encoding'

/**
 * Palette slot assignments used by procedural generation. Must match the palette
 * seeded into RegionMeta in GenerateTerrainDialog.
 */
export const GEN_SLOT = {
  GRASS: 0,
  SAND: 1,
  LATERITE: 2,
  SNOW: 3,
} as const

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

export function computeCoastDistance(heightField: Float32Array): Float32Array {
  const N = REGION_CELLS
  const total = N * N
  const dist = new Float32Array(total)
  dist.fill(Infinity)

  const queue = new Uint32Array(total)
  const inQueue = new Uint8Array(total)
  let head = 0
  let tail = 0

  for (let i = 0; i < total; i++) {
    if (heightField[i] < 0) {
      dist[i] = 0
      queue[tail++] = i
      inQueue[i] = 1
    }
  }

  while (head < tail) {
    const cur = queue[head++]
    inQueue[cur] = 0
    const cx = cur % N
    const cz = Math.floor(cur / N)
    const curDist = dist[cur]

    for (let dz = -1; dz <= 1; dz++) {
      for (let dx = -1; dx <= 1; dx++) {
        if (dx === 0 && dz === 0) continue
        const nx = cx + dx
        const nz = cz + dz
        if (nx < 0 || nx >= N || nz < 0 || nz >= N) continue
        const ni = nz * N + nx
        const newDist = curDist + (dx !== 0 && dz !== 0 ? 1.414 : 1)
        if (newDist < dist[ni]) {
          dist[ni] = newDist
          if (!inQueue[ni]) {
            queue[tail++] = ni
            inQueue[ni] = 1
          }
        }
      }
    }
  }

  return dist
}

export function generateSplatMap(
  heightField: Float32Array,
  coastDist: Float32Array,
  config: TerrainGenConfig,
  regionX: number,
  regionZ: number
): Uint8Array {
  const N = REGION_CELLS
  const splatField = new Uint8Array(N * N * BYTES_PER_CELL)
  const SAND_BAND = 12
  const SAND_HEIGHT_MAX = 0.9

  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const i = cz * N + cx
      const pi = i * BYTES_PER_CELL
      const h = heightField[i]
      const dist = coastDist[i]

      const hL = cx > 0 ? heightField[i - 1] : h
      const hR = cx < N - 1 ? heightField[i + 1] : h
      const hU = cz > 0 ? heightField[i - N] : h
      const hD = cz < N - 1 ? heightField[i + N] : h
      const slope = Math.sqrt((hR - hL) * (hR - hL) + (hD - hU) * (hD - hU)) / 2

      let primary: number = GEN_SLOT.GRASS
      let secondary: number = GEN_SLOT.GRASS
      let blend = 0

      if (h < 0) {
        primary = GEN_SLOT.SAND
        secondary = GEN_SLOT.SAND
      } else if (dist < SAND_BAND && h < SAND_HEIGHT_MAX) {
        const distFactor = 1.0 - dist / SAND_BAND
        const heightFactor = 1.0 - smoothstep(0, SAND_HEIGHT_MAX, h)
        const sandFactor = distFactor * heightFactor
        secondary = GEN_SLOT.SAND
        blend = Math.round(sandFactor * 255)
      } else if (slope > 1.5) {
        secondary = GEN_SLOT.LATERITE
        blend = Math.round(smoothstep(1.5, 3.0, slope) * 255)
      } else if (h > SNOW_START_HEIGHT) {
        secondary = GEN_SLOT.SNOW
        blend = Math.round(
          smoothstep(SNOW_START_HEIGHT, SNOW_FULL_HEIGHT, h) * 255
        )
      }

      splatField[pi + 0] = packIndices(primary, secondary)
      splatField[pi + 1] = 0
      splatField[pi + 2] = blend
      splatField[pi + 3] = 0
    }
  }

  const grassMask = buildGrassMask(splatField, N * N)
  const densityGrid = new Uint8Array(N * N)
  const typeGrid = new Uint8Array(N * N)
  scatterGrassCircles(
    N,
    N,
    regionX * N,
    regionZ * N,
    grassMask,
    densityGrid,
    typeGrid,
    config.seed
  )

  for (let i = 0; i < N * N; i++) {
    splatField[i * BYTES_PER_CELL + VEGMETA_OFFSET] = writeGrass(
      densityGrid[i],
      typeGrid[i] === 1
    )
  }

  return splatField
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
      unpackPrimary(splatField[pi]) === GEN_SLOT.GRASS &&
      splatField[pi + 2] <= GRASS_BLEND_MAX
    ) {
      mask[i] = 1
    }
  }
  return mask
}
