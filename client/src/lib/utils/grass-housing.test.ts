import { describe, it, expect } from 'vitest'
import { removeGrassInRect, type GrassPlacementData } from './grass-data'

// ── helpers ──────────────────────────────────────────────────

const FLOATS_PER_INSTANCE = 5 // x, y, z, rotation, scale
const HEADER_BYTES = 12

/** Build a GrassPlacementData from a list of [x, y, z] positions.
 *  All instances are placed as "short" grass with rotation=0, scale=1. */
function makeGrassData(
  positions: [number, number, number][]
): GrassPlacementData {
  const shortCount = positions.length
  const totalFloats = shortCount * FLOATS_PER_INSTANCE
  const buffer = new ArrayBuffer(HEADER_BYTES + totalFloats * 4)
  const header = new Uint32Array(buffer, 0, 3)
  header[0] = shortCount
  header[1] = 0 // tallCount
  header[2] = 0 // flowerCount
  const body = new Float32Array(buffer, HEADER_BYTES)
  for (let i = 0; i < positions.length; i++) {
    const base = i * FLOATS_PER_INSTANCE
    body[base] = positions[i][0]
    body[base + 1] = positions[i][1]
    body[base + 2] = positions[i][2]
    body[base + 3] = 0 // rotation
    body[base + 4] = 1 // scale
  }
  return { shortCount, tallCount: 0, flowerCount: 0, buffer }
}

/** Extract short-grass X,Z positions from a GrassPlacementData. */
function getPositions(data: GrassPlacementData): [number, number][] {
  const body = new Float32Array(data.buffer, HEADER_BYTES)
  const result: [number, number][] = []
  for (let i = 0; i < data.shortCount; i++) {
    const base = i * FLOATS_PER_INSTANCE
    result.push([body[base], body[base + 2]])
  }
  return result
}

// ── tests ────────────────────────────────────────────────────

describe('removeGrassInRect', () => {
  it('removes grass instances inside the rectangle', () => {
    const data = makeGrassData([
      [5, 0, 5], // inside [0,0]→[10,10]
      [15, 0, 15], // outside
      [8, 0, 3], // inside
    ])

    const filtered = removeGrassInRect(data, 0, 0, 10, 10)
    expect(filtered).not.toBeNull()
    expect(filtered!.shortCount).toBe(1)
    expect(getPositions(filtered!)).toEqual([[15, 15]])
  })

  it('returns null when no instances are removed', () => {
    const data = makeGrassData([
      [15, 0, 15],
      [20, 0, 20],
    ])

    const filtered = removeGrassInRect(data, 0, 0, 10, 10)
    expect(filtered).toBeNull()
  })

  it('removes all instances when all are inside', () => {
    const data = makeGrassData([
      [5, 0, 5],
      [8, 0, 8],
    ])

    const filtered = removeGrassInRect(data, 0, 0, 10, 10)
    expect(filtered).not.toBeNull()
    expect(filtered!.shortCount).toBe(0)
  })

  it('includes boundary instances (inclusive bounds)', () => {
    const data = makeGrassData([
      [0, 0, 0], // on minX, minZ boundary
      [10, 0, 10], // on maxX, maxZ boundary
      [11, 0, 11], // outside
    ])

    const filtered = removeGrassInRect(data, 0, 0, 10, 10)
    expect(filtered).not.toBeNull()
    expect(filtered!.shortCount).toBe(1)
    expect(getPositions(filtered!)).toEqual([[11, 11]])
  })
})

describe('grass suppression under house footprint', () => {
  it('removes grass within house footprint + margin', () => {
    // Simulate a house at origin (10, 0, 10) with a 4×4 ground-floor room
    const houseOriginX = 10
    const houseOriginZ = 10
    const roomSizeX = 4
    const roomSizeZ = 4
    const GRASS_MARGIN = 1

    const rectMinX = houseOriginX - GRASS_MARGIN // 9
    const rectMinZ = houseOriginZ - GRASS_MARGIN // 9
    const rectMaxX = houseOriginX + roomSizeX + GRASS_MARGIN // 15
    const rectMaxZ = houseOriginZ + roomSizeZ + GRASS_MARGIN // 15

    const data = makeGrassData([
      [12, 0, 12], // inside house
      [10, 0, 10], // inside house (corner)
      [14, 0, 14], // inside margin
      [8, 0, 8], // outside
      [20, 0, 20], // outside
      [9, 0, 12], // inside margin zone
    ])

    const filtered = removeGrassInRect(
      data,
      rectMinX,
      rectMinZ,
      rectMaxX,
      rectMaxZ
    )
    expect(filtered).not.toBeNull()
    expect(filtered!.shortCount).toBe(2)
    expect(getPositions(filtered!)).toEqual([
      [8, 8],
      [20, 20],
    ])
  })

  it('sequential removal for multiple rooms clears all footprints', () => {
    // Two adjacent 4×4 rooms
    const GRASS_MARGIN = 1
    const rooms = [
      { x: 10, z: 10, sx: 4, sz: 4 },
      { x: 14, z: 10, sx: 4, sz: 4 },
    ]

    let data: GrassPlacementData = makeGrassData([
      [12, 0, 12], // inside room 1
      [16, 0, 12], // inside room 2
      [25, 0, 25], // outside both
    ])

    for (const room of rooms) {
      const result = removeGrassInRect(
        data,
        room.x - GRASS_MARGIN,
        room.z - GRASS_MARGIN,
        room.x + room.sx + GRASS_MARGIN,
        room.z + room.sz + GRASS_MARGIN
      )
      if (result) data = result
    }

    expect(data.shortCount).toBe(1)
    expect(getPositions(data)).toEqual([[25, 25]])
  })

  it('resplat scenario: regenerated grass is cleared under existing houses', () => {
    // This is the exact bug scenario:
    // 1. Grass is regenerated (resplat) — fills entire tile including house area
    // 2. removeGrassInRect must be called for each ground-floor room
    // 3. Result: no grass inside house footprints

    const GRASS_MARGIN = 1
    const house = {
      origin: { x: 20, z: 20 },
      rooms: [
        {
          localX: 0,
          localZ: 0,
          sizeX: 6,
          sizeZ: 6,
          floorLevel: 0,
          roomType: 'normal',
        },
        {
          localX: 0,
          localZ: 0,
          sizeX: 6,
          sizeZ: 6,
          floorLevel: 1,
          roomType: 'normal',
        },
        {
          localX: 6,
          localZ: 0,
          sizeX: 3,
          sizeZ: 3,
          floorLevel: 0,
          roomType: 'stairwell',
        },
      ],
    }

    // Simulated resplat output: grass everywhere including inside house
    // Stairwell room: origin (26, 20), size 3×3 → world [26,20]→[29,23]
    // Normal room: origin (20, 20), size 6×6 → world [20,20]→[26,26]
    // With margin=1, normal room removal rect: [19,19]→[27,27]
    // Place stairwell instance at (28, 21) — inside stairwell but outside normal room+margin
    let data: GrassPlacementData = makeGrassData([
      [5, 0, 5], // far from house
      [22, 0, 22], // inside house room 0
      [15, 0, 15], // outside house
      [24, 0, 24], // inside house room 0
      [35, 0, 35], // far from house
      [28, 0, 21], // inside stairwell-only area (should be kept)
    ])

    // Apply grass removal for ground-floor non-stairwell rooms (same logic as production)
    for (const room of house.rooms) {
      if (room.floorLevel !== 0 || room.roomType === 'stairwell') continue
      const minX = house.origin.x + room.localX - GRASS_MARGIN
      const minZ = house.origin.z + room.localZ - GRASS_MARGIN
      const maxX = house.origin.x + room.localX + room.sizeX + GRASS_MARGIN
      const maxZ = house.origin.z + room.localZ + room.sizeZ + GRASS_MARGIN

      const filtered = removeGrassInRect(data, minX, minZ, maxX, maxZ)
      if (filtered) data = filtered
    }

    // Grass inside the ground-floor normal room is removed
    // Grass in stairwell area, upper floors, and outside remain
    expect(data.shortCount).toBe(4)
    const positions = getPositions(data)
    expect(positions).toContainEqual([5, 5])
    expect(positions).toContainEqual([15, 15])
    expect(positions).toContainEqual([35, 35])
    expect(positions).toContainEqual([28, 21]) // stairwell area not cleared

    // The bug: without removeGrassInRect after resplat, (22,22) and (24,24) would remain
    expect(positions).not.toContainEqual([22, 22])
    expect(positions).not.toContainEqual([24, 24])
  })

  it('handles empty grass data', () => {
    const data = makeGrassData([])
    const filtered = removeGrassInRect(data, 0, 0, 10, 10)
    expect(filtered).toBeNull()
  })

  it('works with negative coordinates', () => {
    const data = makeGrassData([
      [-15, 0, -20], // inside [-20,-25]→[-5,-10]
      [-3, 0, -8], // outside
      [-10, 0, -15], // inside
    ])

    const filtered = removeGrassInRect(data, -20, -25, -5, -10)
    expect(filtered).not.toBeNull()
    expect(filtered!.shortCount).toBe(1)
    expect(getPositions(filtered!)).toEqual([[-3, -8]])
  })
})
