import { createRng } from '../utils/simplex-noise'
import {
  REGION_CELLS,
  smoothstep,
  type TerrainGenConfig,
} from './terrain-constants'
import {
  BYTES_PER_CELL,
  GRASS_DENSITY_LEVELS,
  SHORT_GRASS_MIN,
  TALL_GRASS_MIN,
  packIndices,
  unpackPrimary,
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
const GRASS_BLEND_MAX = 26

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
  const snowStart = config.maxHeight * 0.7
  const snowFull = config.maxHeight * 0.85

  const TALL_GRASS_PROB = 0.3

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
      } else if (h > snowStart && config.maxHeight > 20) {
        secondary = GEN_SLOT.SNOW
        blend = Math.round(smoothstep(snowStart, snowFull, h) * 255)
      }

      splatField[pi + 0] = packIndices(primary, secondary)
      splatField[pi + 1] = 0
      splatField[pi + 2] = blend
      splatField[pi + 3] = 0
    }
  }

  // ── Grass circle scatter (world-space deterministic) ──
  // A cell is grass-eligible if primary is grass and secondary weight is low.
  const grassMask = new Uint8Array(N * N)
  for (let i = 0; i < N * N; i++) {
    const pi = i * BYTES_PER_CELL
    const primary = unpackPrimary(splatField[pi])
    const blend = splatField[pi + 2]
    if (primary === GEN_SLOT.GRASS && blend <= GRASS_BLEND_MAX) {
      grassMask[i] = 1
    }
  }

  const densityGrid = new Uint8Array(N * N)
  const typeGrid = new Uint8Array(N * N)
  const CIRCLE_RADII = [5, 7, 8, 10, 12, 15]
  const MAX_RADIUS = 15
  const SCATTER_CELL = 64
  const CIRCLES_PER_SCATTER = 20
  const DENSITY_MAX = GRASS_DENSITY_LEVELS - 1

  const regionOX = regionX * N
  const regionOZ = regionZ * N

  const scMinX = Math.floor((regionOX - MAX_RADIUS) / SCATTER_CELL)
  const scMaxX = Math.floor((regionOX + N - 1 + MAX_RADIUS) / SCATTER_CELL)
  const scMinZ = Math.floor((regionOZ - MAX_RADIUS) / SCATTER_CELL)
  const scMaxZ = Math.floor((regionOZ + N - 1 + MAX_RADIUS) / SCATTER_CELL)

  for (let scz = scMinZ; scz <= scMaxZ; scz++) {
    for (let scx = scMinX; scx <= scMaxX; scx++) {
      const cellSeed =
        (config.seed ^ 0x47524153) +
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

        const lcx = Math.floor(wcx) - regionOX
        const lcz = Math.floor(wcz) - regionOZ
        const centerInRegion = lcx >= 0 && lcx < N && lcz >= 0 && lcz < N
        if (centerInRegion && !grassMask[lcz * N + lcx]) continue

        const lMinX = Math.max(0, Math.floor(wcx - radius) - regionOX)
        const lMaxX = Math.min(N - 1, Math.ceil(wcx + radius) - regionOX)
        const lMinZ = Math.max(0, Math.floor(wcz - radius) - regionOZ)
        const lMaxZ = Math.min(N - 1, Math.ceil(wcz + radius) - regionOZ)
        if (lMinX > N - 1 || lMaxX < 0 || lMinZ > N - 1 || lMaxZ < 0) continue

        const r2 = radius * radius
        for (let z = lMinZ; z <= lMaxZ; z++) {
          for (let x = lMinX; x <= lMaxX; x++) {
            const wx = regionOX + x
            const wz = regionOZ + z
            const dx = wx - wcx
            const dz = wz - wcz
            if (dx * dx + dz * dz > r2) continue
            const idx = z * N + x
            if (!grassMask[idx]) continue

            const ddist = Math.sqrt(dx * dx + dz * dz)
            const falloff = 1.0 - smoothstep(radius * 0.3, radius, ddist)
            const d = Math.round(circleDensity * falloff)
            if (d > densityGrid[idx]) {
              densityGrid[idx] = d
              typeGrid[idx] = isTall ? 1 : 0
            }
          }
        }
      }
    }
  }

  // Write density + subtype into vegMeta byte
  for (let i = 0; i < N * N; i++) {
    if (densityGrid[i] > 0) {
      const base = typeGrid[i] === 1 ? TALL_GRASS_MIN : SHORT_GRASS_MIN
      splatField[i * BYTES_PER_CELL + 3] =
        base + Math.min(densityGrid[i], DENSITY_MAX)
    }
  }

  return splatField
}
