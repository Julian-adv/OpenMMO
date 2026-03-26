import { describe, it, expect } from 'vitest'
import {
  tileSubChunkRange,
  isKeyInTileRange,
  partitionKeysFromRawData,
  filterKeysToTileRange,
} from './grass-sub-chunks'

// TERRAIN_TILE_SIZE = 64, SUB_CHUNK_SIZE = 32
// Tile (0,0) covers world X [-32, 32), Z [-32, 32)
// → sub-chunks: (-1,-1), (-1,0), (0,-1), (0,0)

describe('tileSubChunkRange', () => {
  it('computes correct range for tile (0,0)', () => {
    const r = tileSubChunkRange(0, 0)
    expect(r).toEqual({ scMinX: -1, scMaxX: 0, scMinZ: -1, scMaxZ: 0 })
  })

  it('computes correct range for tile (1,0)', () => {
    const r = tileSubChunkRange(1, 0)
    expect(r).toEqual({ scMinX: 1, scMaxX: 2, scMinZ: -1, scMaxZ: 0 })
  })

  it('tiles do not overlap in sub-chunk ranges', () => {
    const r00 = tileSubChunkRange(0, 0)
    const r10 = tileSubChunkRange(1, 0)
    // tile (0,0) max X should be less than tile (1,0) min X
    expect(r00.scMaxX).toBeLessThan(r10.scMinX)
  })
})

describe('isKeyInTileRange', () => {
  const range = tileSubChunkRange(0, 0) // [-1,0] × [-1,0]

  it('accepts keys within range', () => {
    expect(isKeyInTileRange('-1,-1', range)).toBe(true)
    expect(isKeyInTileRange('0,0', range)).toBe(true)
    expect(isKeyInTileRange('-1,0', range)).toBe(true)
  })

  it('rejects keys outside range', () => {
    expect(isKeyInTileRange('1,0', range)).toBe(false)
    expect(isKeyInTileRange('0,1', range)).toBe(false)
    expect(isKeyInTileRange('-2,-1', range)).toBe(false)
  })
})

describe('partitionKeysFromRawData', () => {
  it('assigns instances to correct sub-chunk keys', () => {
    // Instance at world (10, 0, 5) → sub-chunk floor(10/32),floor(5/32) = "0,0"
    // Instance at world (-20, 0, -10) → sub-chunk "-1,-1"
    const rawData = new Float32Array([
      10,
      0,
      5,
      0,
      1, // → "0,0"
      -20,
      0,
      -10,
      0,
      1, // → "-1,-1"
    ])
    const groups = partitionKeysFromRawData(rawData)
    expect(groups.get('0,0')).toEqual([0])
    expect(groups.get('-1,-1')).toEqual([1])
  })

  it('handles boundary spillover from jitter', () => {
    // Tile (0,0) covers world X [-32, 32).
    // An instance at exactly x=32.0 (boundary) falls into sub-chunk 1,
    // which belongs to tile (1,0), not tile (0,0).
    const rawData = new Float32Array([
      32.0,
      0,
      0,
      0,
      1, // → floor(32/32) = "1,0" (spillover!)
    ])
    const groups = partitionKeysFromRawData(rawData)
    expect(groups.has('1,0')).toBe(true)
    expect(groups.has('0,0')).toBe(false)
  })
})

describe('filterKeysToTileRange', () => {
  it('keeps keys within tile range and discards spillover', () => {
    // Simulate tile (0,0) data with one spillover key
    const chunks = new Map<string, number>([
      ['-1,-1', 5000], // valid: within tile (0,0)
      ['-1,0', 3000], // valid
      ['0,-1', 4000], // valid
      ['0,0', 6000], // valid
      ['1,0', 2], // spillover: belongs to tile (1,0)
      ['0,1', 1], // spillover: belongs to tile (0,1)
    ])

    const filtered = filterKeysToTileRange(chunks, 0, 0)
    expect(filtered.size).toBe(4)
    expect(filtered.has('-1,-1')).toBe(true)
    expect(filtered.has('0,0')).toBe(true)
    expect(filtered.has('1,0')).toBe(false) // spillover removed
    expect(filtered.has('0,1')).toBe(false) // spillover removed
  })
})

describe('tile boundary overwrite prevention', () => {
  it('second tile loading does not overwrite first tile data', () => {
    // This simulates the exact bug that was fixed:
    // 1. Tile (0,0) loads → sub-chunk "0,0" gets 50000 instances
    // 2. Tile (1,0) loads → has 2 spillover instances in "0,0"
    // 3. Without filtering, tile (1,0) overwrites "0,0" with 2 instances

    const cache = new Map<string, number>()

    // Tile (0,0) loads: partition produces these keys
    const tile00data = new Map<string, number>([
      ['-1,-1', 40000],
      ['-1,0', 35000],
      ['0,-1', 42000],
      ['0,0', 50000],
    ])
    const tile00filtered = filterKeysToTileRange(tile00data, 0, 0)
    for (const [k, v] of tile00filtered) cache.set(k, v)

    // Tile (1,0) loads: has spillover into "0,0"
    const tile10data = new Map<string, number>([
      ['1,-1', 38000],
      ['1,0', 45000],
      ['2,-1', 30000],
      ['2,0', 41000],
      ['0,0', 2], // spillover from boundary jitter!
      ['0,-1', 1], // another spillover
    ])
    const tile10filtered = filterKeysToTileRange(tile10data, 1, 0)
    for (const [k, v] of tile10filtered) cache.set(k, v)

    // The critical assertion: tile (0,0)'s "0,0" data was NOT overwritten
    expect(cache.get('0,0')).toBe(50000)
    expect(cache.get('0,-1')).toBe(42000)
    // Tile (1,0)'s own data is present
    expect(cache.get('1,0')).toBe(45000)
    expect(cache.get('2,0')).toBe(41000)
  })
})
