import { describe, it, expect, vi, beforeEach } from 'vitest'

const removed: string[] = []

vi.mock('../wasm/onlinerpg_shared', () => ({
  // One sealed piece back means "this region left a cache entry behind",
  // which is what evictDistant tracks.
  passability_set_furniture: (_key: string, placements: unknown[]) =>
    placements.length > 0 ? [{ cells: [[0, 0]], yBase: 0 }] : [],
  passability_remove_furniture: (key: string) => removed.push(key),
  furniture_is_solid: () => true,
}))

const { furnitureManager } = await import('./furnitureManager')

/** A region that registers a cache entry (any non-empty placement list). */
function load(rx: number, rz: number) {
  furnitureManager.syncRegion(rx, rz, [{}] as never)
}

describe('furnitureManager.evictDistant', () => {
  beforeEach(() => {
    removed.length = 0
    // Collapse whatever a previous test loaded down to nothing.
    furnitureManager.evictDistant(9999, 9999)
    removed.length = 0
  })

  it('drops regions more than one away and keeps the 8 neighbours', () => {
    load(0, 0)
    load(1, 0) // neighbour
    load(-1, 1) // diagonal neighbour
    load(5, 5) // far
    load(0, 3) // far on one axis only

    furnitureManager.evictDistant(0, 0)

    expect(removed.sort()).toEqual(['furniture:0,3', 'furniture:5,5'])
  })

  it('clears every registered collision region on a world reset', () => {
    load(0, 0)
    load(1, 0)
    furnitureManager.reset()
    expect(removed.sort()).toEqual(['furniture:0,0', 'furniture:1,0'])
    expect(furnitureManager.getDebugPieces()).toEqual([])
  })

  it('keeps furniture across a region boundary the player just crossed', () => {
    // The reason neighbours survive: standing just inside region 1, furniture a
    // metre back in region 0 must still collide, or the client would predict
    // movement the server refuses.
    load(0, 0)
    furnitureManager.evictDistant(1, 0)
    expect(removed).toEqual([])
  })

  it('does not re-remove a region already evicted', () => {
    load(5, 5)
    furnitureManager.evictDistant(0, 0)
    furnitureManager.evictDistant(0, 0)
    expect(removed).toEqual(['furniture:5,5'])
  })

  it('forgets a region that stops contributing solid furniture', () => {
    load(5, 5)
    furnitureManager.syncRegion(5, 5, []) // all pieces deleted in the editor
    furnitureManager.evictDistant(0, 0)
    // wasm already dropped the entry on the empty sync; no second removal.
    expect(removed).toEqual([])
  })
})
