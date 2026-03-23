import { createRng } from '../utils/simplex-noise'
import { SHORT_GRASS_R_MIN, TALL_GRASS_R_MIN } from '../shaders/grass-material'
import {
  REGION_CELLS,
  smoothstep,
  type TerrainGenConfig,
} from './terrain-constants'

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
  const CHANNELS = 4
  const splatField = new Uint8Array(N * N * CHANNELS)
  const SAND_BAND = 12
  const SAND_HEIGHT_MAX = 0.9
  const snowStart = config.maxHeight * 0.7
  const snowFull = config.maxHeight * 0.85

  const SUBTYPE_DENSITY_RANGE = TALL_GRASS_R_MIN - SHORT_GRASS_R_MIN - 1
  const TALL_GRASS_PROB = 0.3

  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const i = cz * N + cx
      const pi = i * CHANNELS
      const h = heightField[i]
      const dist = coastDist[i]

      // Compute slope (central differences)
      const hL = cx > 0 ? heightField[i - 1] : h
      const hR = cx < N - 1 ? heightField[i + 1] : h
      const hU = cz > 0 ? heightField[i - N] : h
      const hD = cz < N - 1 ? heightField[i + N] : h
      const slope = Math.sqrt((hR - hL) * (hR - hL) + (hD - hU) * (hD - hU)) / 2

      let grass = 0,
        rock = 0,
        sand = 0,
        snow = 0

      if (h < 0) {
        sand = 1.0
      } else if (dist < SAND_BAND && h < SAND_HEIGHT_MAX) {
        const distFactor = 1.0 - dist / SAND_BAND
        const heightFactor = 1.0 - smoothstep(0, SAND_HEIGHT_MAX, h)
        const sandFactor = distFactor * heightFactor
        sand = sandFactor
        grass = 1.0 - sandFactor
      } else if (slope > 1.5) {
        const rockFactor = smoothstep(1.5, 3.0, slope)
        rock = rockFactor
        grass = 1.0 - rockFactor
      } else if (h > snowStart && config.maxHeight > 20) {
        const snowFactor = smoothstep(snowStart, snowFull, h)
        snow = snowFactor
        grass = 1.0 - snowFactor
      } else {
        grass = 1.0
      }

      // Normalize to sum = 255
      const total = grass + rock + sand + snow
      if (total > 0) {
        splatField[pi + 0] = Math.round((grass / total) * 255)
        splatField[pi + 1] = Math.round((rock / total) * 255)
        splatField[pi + 2] = Math.round((sand / total) * 255)
        splatField[pi + 3] = Math.round((snow / total) * 255)
      } else {
        splatField[pi + 0] = 255
      }

      // Fix rounding: ensure sum == 255
      const sum =
        splatField[pi] +
        splatField[pi + 1] +
        splatField[pi + 2] +
        splatField[pi + 3]
      if (sum !== 255) {
        let maxCh = 0
        for (let c = 1; c < 4; c++) {
          if (splatField[pi + c] > splatField[pi + maxCh]) maxCh = c
        }
        splatField[pi + maxCh] += 255 - sum
      }
    }
  }

  // --- Grass circle scatter (world-space deterministic) ---
  const grassMask = new Uint8Array(N * N)
  for (let i = 0; i < N * N; i++) {
    if (splatField[i * CHANNELS] >= SHORT_GRASS_R_MIN) {
      grassMask[i] = 1
      splatField[i * CHANNELS] = SHORT_GRASS_R_MIN - 1
    }
  }

  const densityGrid = new Uint8Array(N * N)
  const typeGrid = new Uint8Array(N * N)
  const CIRCLE_RADII = [5, 7, 8, 10, 12, 15]
  const MAX_RADIUS = 15
  const SCATTER_CELL = 64
  const CIRCLES_PER_SCATTER = 20

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
        const circleDensity = Math.round(
          SUBTYPE_DENSITY_RANGE * (0.6 + rng() * 0.4)
        )

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

  // Write density + subtype back to splatField R channel
  for (let i = 0; i < N * N; i++) {
    if (densityGrid[i] > 0) {
      const base = typeGrid[i] === 1 ? TALL_GRASS_R_MIN : SHORT_GRASS_R_MIN
      splatField[i * CHANNELS] =
        base + Math.min(densityGrid[i], SUBTYPE_DENSITY_RANGE)
    }
  }

  return splatField
}
