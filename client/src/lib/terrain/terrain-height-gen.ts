import { createNoise2D, fbm2D, createRng } from '../utils/simplex-noise'
import { sampleBiomeWeights, sampleLandDensity } from './referenceImageSampler'
import {
  TILE_DIM,
  REGION_CELLS,
  SHALLOW_WATER_THRESHOLD,
  DEEP_WATER_THRESHOLD,
  lerp,
  smoothstep,
  type TerrainGenConfig,
  type NeighborEdgeData,
} from './terrain-constants'

export function generateBaseHeights(
  config: TerrainGenConfig,
  worldOffsetX: number,
  worldOffsetZ: number
): Float32Array {
  const N = REGION_CELLS
  const noise = createNoise2D(config.seed)
  const rawHeights = new Float32Array(N * N)
  const baseFreq = 1 / 512

  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const wx = (worldOffsetX + cx) * baseFreq
      const wz = (worldOffsetZ + cz) * baseFreq
      rawHeights[cz * N + cx] = fbm2D(noise, wx, wz, 6, 2.0, 0.5)
    }
  }

  return rawHeights
}

export function classifyAndRemap(
  rawHeights: Float32Array,
  config: TerrainGenConfig
): Float32Array {
  const N = REGION_CELLS
  const total = N * N
  const result = new Float32Array(total)

  // Sort to find quantile thresholds
  const sorted = new Float32Array(rawHeights)
  sorted.sort()

  // Normalize proportions
  const propSum =
    config.seaProportion + config.plainProportion + config.mountainProportion
  const seaFrac = propSum > 0 ? config.seaProportion / propSum : 0.33
  const plainFrac = propSum > 0 ? config.plainProportion / propSum : 0.34

  // Split sea into deep and shallow zones
  const shallowRatio = Math.max(0, Math.min(1, config.shallowSeaRatio))
  const deepSeaFrac = seaFrac * (1 - shallowRatio)
  const shallowSeaFrac = seaFrac * shallowRatio

  const deepSeaIdx = Math.floor(deepSeaFrac * total)
  const shallowSeaIdx = Math.floor((deepSeaFrac + shallowSeaFrac) * total)
  const plainIdx = Math.floor((seaFrac + plainFrac) * total)

  const deepSeaThreshold =
    deepSeaIdx > 0 ? sorted[deepSeaIdx - 1] : sorted[0] - 1
  const shallowSeaThreshold =
    shallowSeaIdx > 0 ? sorted[shallowSeaIdx - 1] : sorted[0] - 1
  const plainThreshold =
    plainIdx < total ? sorted[plainIdx - 1] : sorted[total - 1] + 1

  const rawMin = sorted[0]
  const rawMax = sorted[total - 1]

  for (let i = 0; i < total; i++) {
    const raw = rawHeights[i]

    if (raw <= deepSeaThreshold) {
      // Deep sea: remap to [minHeight, -1]
      const t =
        deepSeaThreshold > rawMin
          ? (raw - rawMin) / (deepSeaThreshold - rawMin)
          : 0.5
      result[i] = lerp(config.minHeight, DEEP_WATER_THRESHOLD, t)
    } else if (raw <= shallowSeaThreshold) {
      // Shallow sea: remap to [DEEP_WATER_THRESHOLD, SHALLOW_WATER_THRESHOLD]
      const t =
        shallowSeaThreshold > deepSeaThreshold
          ? (raw - deepSeaThreshold) / (shallowSeaThreshold - deepSeaThreshold)
          : 0.5
      result[i] = lerp(DEEP_WATER_THRESHOLD, SHALLOW_WATER_THRESHOLD, t)
    } else if (raw <= plainThreshold) {
      // Plains: remap to [0.5, 10]
      const t =
        plainThreshold > shallowSeaThreshold
          ? (raw - shallowSeaThreshold) / (plainThreshold - shallowSeaThreshold)
          : 0.5
      result[i] = lerp(0.5, 10, t)
    } else {
      // Mountains: remap to [10, maxHeight]
      const t =
        rawMax > plainThreshold
          ? (raw - plainThreshold) / (rawMax - plainThreshold)
          : 0.5
      result[i] = lerp(10, config.maxHeight, t)
    }
  }

  return result
}

export function classifyAndRemapWithReference(
  rawHeights: Float32Array,
  config: TerrainGenConfig,
  worldOffsetX: number,
  worldOffsetZ: number
): Float32Array {
  const N = REGION_CELLS
  const total = N * N
  const result = new Float32Array(total)
  const img = config.referenceImage!

  // Pre-compute quantile-based fallback for cells outside the image
  const fallback = classifyAndRemap(rawHeights, config)

  // --- Pass 1: Initial height assignment (all sea as deep) ---
  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const i = cz * N + cx
      const worldX = worldOffsetX + cx
      const worldZ = worldOffsetZ + cz

      const weights = sampleBiomeWeights(img, worldX, worldZ)
      if (!weights) {
        result[i] = fallback[i]
        continue
      }

      // Normalize noise to [0, 1] (fBm can exceed ±1, so clamp)
      const t = Math.max(0, Math.min(1, (rawHeights[i] + 1) * 0.5))

      // All sea starts as deep
      const seaHeight = lerp(config.minHeight, -1, t)

      // River: shallow negative height (carved channel)
      const riverHeight = lerp(-2.0, -0.5, t)

      const refHeight =
        weights.sea * seaHeight +
        weights.plains * lerp(0.5, 25, t) +
        weights.mountain * lerp(10, config.maxHeight, t) +
        weights.highland * lerp(config.maxHeight * 0.7, config.maxHeight, t) +
        weights.river * riverHeight

      result[i] = refHeight
    }
  }

  // --- Pass 2: Compute land density from reference image ---
  const DENSITY_PIXEL_RADIUS = 10
  const densityGridSize = Math.ceil(N / TILE_DIM) + 1
  const densityGrid = new Float32Array(densityGridSize * densityGridSize)
  for (let gz = 0; gz < densityGridSize; gz++) {
    for (let gx = 0; gx < densityGridSize; gx++) {
      const wx = worldOffsetX + gx * TILE_DIM
      const wz = worldOffsetZ + gz * TILE_DIM
      densityGrid[gz * densityGridSize + gx] = sampleLandDensity(
        img,
        wx,
        wz,
        DENSITY_PIXEL_RADIUS
      )
    }
  }
  // Bilinear interpolation of density per cell
  const landDensity = new Float32Array(total)
  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const gx = cx / TILE_DIM
      const gz = cz / TILE_DIM
      const gx0 = Math.min(Math.floor(gx), densityGridSize - 2)
      const gz0 = Math.min(Math.floor(gz), densityGridSize - 2)
      const fx = gx - gx0
      const fz = gz - gz0
      const d00 = densityGrid[gz0 * densityGridSize + gx0]
      const d10 = densityGrid[gz0 * densityGridSize + gx0 + 1]
      const d01 = densityGrid[(gz0 + 1) * densityGridSize + gx0]
      const d11 = densityGrid[(gz0 + 1) * densityGridSize + gx0 + 1]
      landDensity[cz * N + cx] =
        d00 * (1 - fx) * (1 - fz) +
        d10 * fx * (1 - fz) +
        d01 * (1 - fx) * fz +
        d11 * fx * fz
    }
  }

  // --- Pass 3: BFS from coastline, propagating distance AND coast density ---
  const SHALLOW_MIN = 8
  const SHALLOW_MAX = 36
  const landDist = new Float32Array(total)
  landDist.fill(Infinity)
  const coastDensity = new Float32Array(total)

  const queue = new Uint32Array(total * 2)
  const inQueue = new Uint8Array(total)
  let head = 0
  let tail = 0

  // Seed BFS from coastline cells (sea cells adjacent to land)
  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const i = cz * N + cx
      if (result[i] >= 0) continue
      let adjacentToLand = false
      for (let dz = -1; dz <= 1 && !adjacentToLand; dz++) {
        for (let dx = -1; dx <= 1 && !adjacentToLand; dx++) {
          if (dx === 0 && dz === 0) continue
          const nx = cx + dx
          const nz = cz + dz
          if (nx < 0 || nx >= N || nz < 0 || nz >= N) continue
          if (result[nz * N + nx] >= 0) adjacentToLand = true
        }
      }
      if (adjacentToLand) {
        landDist[i] = 0
        coastDensity[i] = landDensity[i]
        queue[tail++] = i
        inQueue[i] = 1
      }
    }
  }

  while (head < tail) {
    const cur = queue[head++]
    inQueue[cur] = 0
    const cx = cur % N
    const cz = Math.floor(cur / N)
    const curDist = landDist[cur]

    for (let dz = -1; dz <= 1; dz++) {
      for (let dx = -1; dx <= 1; dx++) {
        if (dx === 0 && dz === 0) continue
        const nx = cx + dx
        const nz = cz + dz
        if (nx < 0 || nx >= N || nz < 0 || nz >= N) continue
        const ni = nz * N + nx
        if (result[ni] >= 0) continue
        const newDist = curDist + (dx !== 0 && dz !== 0 ? 1.414 : 1)
        if (newDist < landDist[ni]) {
          landDist[ni] = newDist
          coastDensity[ni] = coastDensity[cur]
          if (!inQueue[ni]) {
            queue[tail++] = ni
            inQueue[ni] = 1
          }
        }
      }
    }
  }

  // Remap sea cells near land to shallow, using coastline's density
  for (let i = 0; i < total; i++) {
    if (result[i] >= 0) continue
    const dist = landDist[i]
    if (dist === Infinity) continue
    const density = coastDensity[i]
    let shallowDist: number
    if (density <= 0.4) {
      const t = Math.max(0, density / 0.4)
      shallowDist = lerp(2, SHALLOW_MIN, t)
    } else {
      const t = Math.min(1, (density - 0.4) / 0.2)
      shallowDist = lerp(SHALLOW_MIN, SHALLOW_MAX, t)
    }
    if (dist < shallowDist) {
      const t = dist / shallowDist
      result[i] = lerp(
        SHALLOW_WATER_THRESHOLD,
        DEEP_WATER_THRESHOLD,
        smoothstep(0, 1, t)
      )
    } else {
      const t = Math.min(1, (dist - shallowDist) / SHALLOW_MAX)
      result[i] = lerp(
        DEEP_WATER_THRESHOLD,
        config.minHeight,
        smoothstep(0, 1, t)
      )
    }
  }

  // --- Pass 3b: Smooth land-side coastal slope ---
  const COASTAL_BLEND_DIST = 24
  const COASTAL_TARGET_HEIGHT = 0.05
  const seaDist = new Float32Array(total)
  seaDist.fill(Infinity)

  const landQueue = new Uint32Array(total * 2)
  const landInQueue = new Uint8Array(total)
  let landHead = 0
  let landTail = 0

  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const i = cz * N + cx
      if (result[i] < 0) continue
      let adjacentToSea = false
      for (let dz = -1; dz <= 1 && !adjacentToSea; dz++) {
        for (let dx = -1; dx <= 1 && !adjacentToSea; dx++) {
          if (dx === 0 && dz === 0) continue
          const nx = cx + dx
          const nz = cz + dz
          if (nx < 0 || nx >= N || nz < 0 || nz >= N) continue
          if (result[nz * N + nx] < 0) adjacentToSea = true
        }
      }
      if (adjacentToSea) {
        seaDist[i] = 0
        landQueue[landTail++] = i
        landInQueue[i] = 1
      }
    }
  }

  while (landHead < landTail) {
    const cur = landQueue[landHead++]
    landInQueue[cur] = 0
    const cx = cur % N
    const cz = Math.floor(cur / N)
    const curDist = seaDist[cur]
    if (curDist >= COASTAL_BLEND_DIST) continue

    for (let dz = -1; dz <= 1; dz++) {
      for (let dx = -1; dx <= 1; dx++) {
        if (dx === 0 && dz === 0) continue
        const nx = cx + dx
        const nz = cz + dz
        if (nx < 0 || nx >= N || nz < 0 || nz >= N) continue
        const ni = nz * N + nx
        if (result[ni] < 0) continue
        const newDist = curDist + (dx !== 0 && dz !== 0 ? 1.414 : 1)
        if (newDist < seaDist[ni]) {
          seaDist[ni] = newDist
          if (!landInQueue[ni]) {
            landQueue[landTail++] = ni
            landInQueue[ni] = 1
          }
        }
      }
    }
  }

  // Blend land heights: near coast → low, far from coast → original
  for (let i = 0; i < total; i++) {
    if (result[i] < 0) continue
    const d = seaDist[i]
    if (d >= COASTAL_BLEND_DIST) continue
    const t = smoothstep(0, 1, d / COASTAL_BLEND_DIST)
    result[i] = lerp(COASTAL_TARGET_HEIGHT, result[i], t)
  }

  return result
}

export function carveRivers(
  heightField: Float32Array,
  config: TerrainGenConfig
) {
  if (config.riverCount <= 0) return

  const N = REGION_CELLS
  const rng = createRng(config.seed + 7919)

  // Collect mountain candidates (height > 15m)
  const candidates: number[] = []
  for (let i = 0; i < N * N; i++) {
    if (heightField[i] > 15) candidates.push(i)
  }
  if (candidates.length === 0) return

  // Shuffle candidates
  for (let i = candidates.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1))
    const tmp = candidates[i]
    candidates[i] = candidates[j]
    candidates[j] = tmp
  }

  const numRivers = Math.min(config.riverCount, candidates.length)

  for (let r = 0; r < numRivers; r++) {
    const start = candidates[r]
    const visited = new Set<number>()
    let current = start
    const path: number[] = []

    // Follow gradient descent to sea level
    while (heightField[current] > 0 && path.length < 2000) {
      path.push(current)
      visited.add(current)

      const cx = current % N
      const cz = Math.floor(current / N)

      // Find lowest neighbor
      let lowestIdx = current
      let lowestH = heightField[current]

      for (let dz = -1; dz <= 1; dz++) {
        for (let dx = -1; dx <= 1; dx++) {
          if (dx === 0 && dz === 0) continue
          const nx = cx + dx
          const nz = cz + dz
          if (nx < 0 || nx >= N || nz < 0 || nz >= N) continue
          const ni = nz * N + nx
          if (visited.has(ni)) continue
          if (heightField[ni] < lowestH) {
            lowestH = heightField[ni]
            lowestIdx = ni
          }
        }
      }

      // Random lateral drift (20% chance)
      if (rng() < 0.2 && path.length > 5) {
        const perpDx = cz > 0 ? 1 : -1
        const perpDz = cx > 0 ? -1 : 1
        const lateralX = cx + perpDx
        const lateralZ = cz + perpDz
        if (lateralX >= 0 && lateralX < N && lateralZ >= 0 && lateralZ < N) {
          const li = lateralZ * N + lateralX
          if (!visited.has(li)) {
            lowestIdx = li
          }
        }
      }

      if (lowestIdx === current) break
      current = lowestIdx
    }

    // Carve channel along path
    const riverWidth = 2
    for (let pi = 0; pi < path.length; pi++) {
      const px = path[pi] % N
      const pz = Math.floor(path[pi] / N)
      const widthFactor = 1 + (pi / path.length) * 1.5
      const w = Math.ceil(riverWidth * widthFactor)

      for (let dz = -w; dz <= w; dz++) {
        for (let dx = -w; dx <= w; dx++) {
          const dist = Math.sqrt(dx * dx + dz * dz)
          if (dist > w) continue
          const nx = px + dx
          const nz = pz + dz
          if (nx < 0 || nx >= N || nz < 0 || nz >= N) continue

          const ni = nz * N + nx
          const depthFactor = Math.exp(-(dist * dist) / (2 * (w / 2) * (w / 2)))
          const carveDepth = 2.0 * depthFactor
          const target = -carveDepth
          heightField[ni] = Math.min(heightField[ni], target)
        }
      }
    }
  }
}

export function blendBoundaries(
  heightField: Float32Array,
  edges: NeighborEdgeData
) {
  const N = REGION_CELLS
  const BLEND_WIDTH = 16

  if (edges.north) {
    for (let cz = 0; cz < BLEND_WIDTH; cz++) {
      const t = cz / BLEND_WIDTH
      for (let cx = 0; cx < N; cx++) {
        const i = cz * N + cx
        const neighborH = edges.north[cx]
        heightField[i] = lerp(neighborH, heightField[i], t)
      }
    }
  }

  if (edges.south) {
    for (let cz = 0; cz < BLEND_WIDTH; cz++) {
      const t = cz / BLEND_WIDTH
      const actualZ = N - 1 - cz
      for (let cx = 0; cx < N; cx++) {
        const i = actualZ * N + cx
        const neighborH = edges.south[cx]
        heightField[i] = lerp(neighborH, heightField[i], t)
      }
    }
  }

  if (edges.west) {
    for (let cx = 0; cx < BLEND_WIDTH; cx++) {
      const t = cx / BLEND_WIDTH
      for (let cz = 0; cz < N; cz++) {
        const i = cz * N + cx
        const neighborH = edges.west[cz]
        heightField[i] = lerp(neighborH, heightField[i], t)
      }
    }
  }

  if (edges.east) {
    for (let cx = 0; cx < BLEND_WIDTH; cx++) {
      const t = cx / BLEND_WIDTH
      const actualX = N - 1 - cx
      for (let cz = 0; cz < N; cz++) {
        const i = cz * N + actualX
        const neighborH = edges.east[cz]
        heightField[i] = lerp(neighborH, heightField[i], t)
      }
    }
  }
}
