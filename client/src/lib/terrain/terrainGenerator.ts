import { createNoise2D, fbm2D, createRng } from '../utils/simplex-noise'

/** Height threshold below which water is considered deep sea (used for minimap coloring) */
export const DEEP_WATER_THRESHOLD = -1.5
import {
  sampleBiomeWeights,
  sampleLandDensity,
  type ReferenceImageData,
} from './referenceImageSampler'

const TILE_DIM = 64
const VERTS_PER_SIDE = TILE_DIM + 1 // 65
const REGION_SIZE = 16
const REGION_CELLS = REGION_SIZE * TILE_DIM // 1024

function encodeHeight(meters: number): number {
  return Math.round((meters + 500.0) / 0.05)
}

export interface TerrainGenConfig {
  seed: number
  minHeight: number // meters (-500 ~ 0)
  maxHeight: number // meters (0 ~ 3276)
  seaProportion: number // 0..1
  plainProportion: number // 0..1
  mountainProportion: number // 0..1
  shallowSeaRatio: number // 0..1, fraction of sea area that is shallow
  riverCount: number // 0..5
  referenceImage?: ReferenceImageData // optional reference image for biome placement
}

export interface GeneratedTile {
  tileX: number
  tileZ: number
  heightmap: Uint16Array // 4225 values (65*65, vertex-based)
  splatmap: Uint8Array // 16384 values (64*64*4, cell-based)
}

export interface NeighborEdgeData {
  north?: Float32Array // 1024 heights (top row of the region above)
  south?: Float32Array // 1024 heights (bottom row of the region below)
  east?: Float32Array // 1024 heights (left column of the region to the right)
  west?: Float32Array // 1024 heights (right column of the region to the left)
}

/**
 * Generate terrain for an entire region (16x16 tiles = 1024x1024 cells).
 */
export function generateRegionTerrain(
  regionX: number,
  regionZ: number,
  config: TerrainGenConfig,
  neighborEdges?: NeighborEdgeData
): GeneratedTile[] {
  const N = REGION_CELLS

  // --- Phase 1: Base elevation via fBm ---
  const noise = createNoise2D(config.seed)
  const rawHeights = new Float32Array(N * N)
  const baseFreq = 1 / 512

  // World offset so noise is continuous across regions
  const worldOffsetX = regionX * N
  const worldOffsetZ = regionZ * N

  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const wx = (worldOffsetX + cx) * baseFreq
      const wz = (worldOffsetZ + cz) * baseFreq
      rawHeights[cz * N + cx] = fbm2D(noise, wx, wz, 6, 2.0, 0.5)
    }
  }

  // --- Phase 2: Classification & height remapping ---
  const heightField = config.referenceImage
    ? classifyAndRemapWithReference(
        rawHeights,
        config,
        worldOffsetX,
        worldOffsetZ
      )
    : classifyAndRemap(rawHeights, config)

  // --- Phase 3: River carving ---
  carveRivers(heightField, config)

  // --- Phase 4: Coast distance (used by splat map) ---
  const coastDist = computeCoastDistance(heightField)

  // --- Phase 5: Region boundary blending ---
  if (neighborEdges) {
    blendBoundaries(heightField, neighborEdges)
  }

  // --- Phase 6: Splat map generation ---
  const splatField = generateSplatMap(heightField, coastDist, config)

  // --- Slice into per-tile data ---
  return sliceIntoTiles(regionX, regionZ, heightField, splatField)
}

function classifyAndRemap(
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
      // Shallow sea: remap to [DEEP_WATER_THRESHOLD, -0.1]
      const t =
        shallowSeaThreshold > deepSeaThreshold
          ? (raw - deepSeaThreshold) / (shallowSeaThreshold - deepSeaThreshold)
          : 0.5
      result[i] = lerp(DEEP_WATER_THRESHOLD, -0.1, t)
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

function classifyAndRemapWithReference(
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

      // Normalize noise to [0, 1]
      const t = (rawHeights[i] + 1) * 0.5

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

  // --- Pass 2: Compute land density from reference image (captures large-scale curvature) ---
  const DENSITY_PIXEL_RADIUS = 10 // pixels in ref image (~320m world)
  // Sample density at tile granularity (64 cells) and interpolate for efficiency
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
  const SHALLOW_MIN = 8 // convex coast (peninsula)
  const SHALLOW_MAX = 48 // concave coast (bay)
  const landDist = new Float32Array(total)
  landDist.fill(Infinity)
  const coastDensity = new Float32Array(total) // density at nearest coastline point

  const queue = new Uint32Array(total * 2)
  const inQueue = new Uint8Array(total)
  let head = 0
  let tail = 0

  // Seed BFS from coastline cells (sea cells adjacent to land)
  for (let cz = 0; cz < N; cz++) {
    for (let cx = 0; cx < N; cx++) {
      const i = cz * N + cx
      if (result[i] >= 0) continue // skip land
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
        if (result[ni] >= 0) continue // don't propagate into land
        const newDist = curDist + (dx !== 0 && dz !== 0 ? 1.414 : 1)
        if (newDist < landDist[ni]) {
          landDist[ni] = newDist
          coastDensity[ni] = coastDensity[cur] // propagate coast's density
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
    if (result[i] >= 0) continue // skip land
    const dist = landDist[i]
    if (dist === Infinity) continue
    // Use the density at the nearest coastline point
    const density = coastDensity[i]
    // density ≈ 0.5 at straight coast, > 0.5 = concave (bay), < 0.5 = convex (peninsula)
    let shallowDist: number
    if (density <= 0.4) {
      // Very convex (peninsula tip): 2 ~ 8
      const t = Math.max(0, density / 0.4) // 0 → 0, 0.4 → 1
      shallowDist = lerp(2, SHALLOW_MIN, t)
    } else {
      // Straight to concave: 8 ~ 48
      const t = Math.min(1, (density - 0.4) / 0.2) // 0.4 → 0, 0.6 → 1
      shallowDist = lerp(SHALLOW_MIN, SHALLOW_MAX, t)
    }
    if (dist < shallowDist) {
      // Shallow zone: -0.1 → DEEP_WATER_THRESHOLD
      const t = dist / shallowDist
      result[i] = lerp(-0.1, DEEP_WATER_THRESHOLD, smoothstep(0, 1, t))
    } else {
      // Deep zone: DEEP_WATER_THRESHOLD → config.minHeight
      const t = Math.min(1, (dist - shallowDist) / shallowDist)
      result[i] = lerp(DEEP_WATER_THRESHOLD, config.minHeight, smoothstep(0, 1, t))
    }
  }

  return result
}

function carveRivers(heightField: Float32Array, config: TerrainGenConfig) {
  if (config.riverCount <= 0) return

  const N = REGION_CELLS
  const rng = createRng(config.seed + 7919) // offset seed for rivers

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

      if (lowestIdx === current) break // stuck
      current = lowestIdx
    }

    // Carve channel along path
    const riverWidth = 2
    for (let pi = 0; pi < path.length; pi++) {
      const px = path[pi] % N
      const pz = Math.floor(path[pi] / N)
      // Width increases toward end of path
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
          // Gaussian cross-section: deeper in center
          const depthFactor = Math.exp(-(dist * dist) / (2 * (w / 2) * (w / 2)))
          const carveDepth = 2.0 * depthFactor
          const target = -carveDepth
          heightField[ni] = Math.min(heightField[ni], target)
        }
      }
    }
  }
}

function computeCoastDistance(heightField: Float32Array): Float32Array {
  const N = REGION_CELLS
  const total = N * N
  const dist = new Float32Array(total)
  dist.fill(Infinity)

  // BFS from all sea cells using fixed-size queue with visited tracking
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


function blendBoundaries(heightField: Float32Array, edges: NeighborEdgeData) {
  const N = REGION_CELLS
  const BLEND_WIDTH = 16

  // North edge (cz = 0): blend with neighbor's bottom row
  if (edges.north) {
    for (let cz = 0; cz < BLEND_WIDTH; cz++) {
      const t = cz / BLEND_WIDTH // 0 at edge, 1 at inner
      for (let cx = 0; cx < N; cx++) {
        const i = cz * N + cx
        const neighborH = edges.north[cx]
        heightField[i] = lerp(neighborH, heightField[i], t)
      }
    }
  }

  // South edge (cz = N-1): blend with neighbor's top row
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

  // West edge (cx = 0): blend with neighbor's right column
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

  // East edge (cx = N-1): blend with neighbor's left column
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

function generateSplatMap(
  heightField: Float32Array,
  coastDist: Float32Array,
  config: TerrainGenConfig
): Uint8Array {
  const N = REGION_CELLS
  const CHANNELS = 4
  const splatField = new Uint8Array(N * N * CHANNELS)
  const SAND_BAND = 12 // cells from water edge
  const SAND_HEIGHT_MAX = 0.9 // meters — sand fades out above this height
  const snowStart = config.maxHeight * 0.7
  const snowFull = config.maxHeight * 0.85

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
        // Underwater: sandy
        sand = 1.0
      } else if (dist < SAND_BAND && h < SAND_HEIGHT_MAX) {
        // Coastline: blend sand with grass (fades by distance and height)
        const distFactor = 1.0 - dist / SAND_BAND
        const heightFactor = 1.0 - smoothstep(0, SAND_HEIGHT_MAX, h)
        const sandFactor = distFactor * heightFactor
        sand = sandFactor
        grass = 1.0 - sandFactor
      } else if (slope > 1.5) {
        // Steep slope: rock
        const rockFactor = smoothstep(1.5, 3.0, slope)
        rock = rockFactor
        grass = 1.0 - rockFactor
      } else if (h > snowStart && config.maxHeight > 20) {
        // High altitude: snow
        const snowFactor = smoothstep(snowStart, snowFull, h)
        snow = snowFactor
        grass = 1.0 - snowFactor
      } else {
        // Default land
        grass = 1.0
      }

      // Normalize to sum = 255
      const total = grass + rock + sand + snow
      if (total > 0) {
        splatField[pi + 0] = Math.round((grass / total) * 255) // R: rocky_terrain
        splatField[pi + 1] = Math.round((rock / total) * 255) // G: gravel_floor
        splatField[pi + 2] = Math.round((sand / total) * 255) // B: sandy_gravel
        splatField[pi + 3] = Math.round((snow / total) * 255) // A: snow
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
        // Add remainder to the dominant channel
        let maxCh = 0
        for (let c = 1; c < 4; c++) {
          if (splatField[pi + c] > splatField[pi + maxCh]) maxCh = c
        }
        splatField[pi + maxCh] += 255 - sum
      }
    }
  }

  return splatField
}

function sliceIntoTiles(
  regionX: number,
  regionZ: number,
  heightField: Float32Array,
  splatField: Uint8Array
): GeneratedTile[] {
  const N = REGION_CELLS
  const tiles: GeneratedTile[] = []
  const baseTileX = regionX * REGION_SIZE
  const baseTileZ = regionZ * REGION_SIZE

  for (let tz = 0; tz < REGION_SIZE; tz++) {
    for (let tx = 0; tx < REGION_SIZE; tx++) {
      const heightmap = new Uint16Array(VERTS_PER_SIDE * VERTS_PER_SIDE)
      const splatmap = new Uint8Array(TILE_DIM * TILE_DIM * 4)

      // Height: 65×65 vertices (overlapping edges with adjacent tiles)
      for (let vz = 0; vz < VERTS_PER_SIDE; vz++) {
        for (let vx = 0; vx < VERTS_PER_SIDE; vx++) {
          const regionCX = Math.min(tx * TILE_DIM + vx, N - 1)
          const regionCZ = Math.min(tz * TILE_DIM + vz, N - 1)
          const ri = regionCZ * N + regionCX
          const ti = vz * VERTS_PER_SIDE + vx

          const h = heightField[ri]
          heightmap[ti] = Math.max(0, Math.min(65535, encodeHeight(h)))
        }
      }

      // Splat: 64×64 cells (unchanged)
      for (let cz = 0; cz < TILE_DIM; cz++) {
        for (let cx = 0; cx < TILE_DIM; cx++) {
          const regionCX = tx * TILE_DIM + cx
          const regionCZ = tz * TILE_DIM + cz
          const ri = regionCZ * N + regionCX
          const ti = cz * TILE_DIM + cx

          const rsi = ri * 4
          const tsi = ti * 4
          splatmap[tsi] = splatField[rsi]
          splatmap[tsi + 1] = splatField[rsi + 1]
          splatmap[tsi + 2] = splatField[rsi + 2]
          splatmap[tsi + 3] = splatField[rsi + 3]
        }
      }

      tiles.push({
        tileX: baseTileX + tx,
        tileZ: baseTileZ + tz,
        heightmap,
        splatmap,
      })
    }
  }

  return tiles
}

// --- Utility functions ---

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t
}

function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)))
  return t * t * (3 - 2 * t)
}
