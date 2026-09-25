import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import {
  applyFightUpdate,
  fishingBobbers,
  fishingCatches,
  FISHING_CATCH_DURATION,
  landFishingCatch,
  removeFishingCatch,
  fishingReelStance,
  markBobberBite,
  myFishing,
  removeBobber,
  resetFishingStore,
  setLocalFishingAction,
  updateBobberFight,
  upsertBobber,
  type FightStatus,
} from './fishingStore'

const ID = 7

describe('catch presentation', () => {
  const fish = { item_def_id: 'raw_trout', size_cm: 42, trophy: false }
  beforeEach(() => {
    vi.useFakeTimers()
    resetFishingStore()
  })
  afterEach(() => {
    resetFishingStore()
    vi.useRealTimers()
  })

  it('replaces the water bobber with a bounded catch and keeps late beats from reviving it', () => {
    upsertBobber(ID, { x: 1, y: 2, z: 3 })
    landFishingCatch(ID, fish)
    expect(get(fishingBobbers).has(ID)).toBe(false)
    expect(get(fishingCatches).get(ID)).toMatchObject({
      fish,
      waterPosition: { x: 1, y: 2, z: 3 },
    })
    updateBobberFight(ID, { x: 4, y: 2, z: 3 }, 'running', 50)
    expect(get(fishingBobbers).has(ID)).toBe(false)
    vi.advanceTimersByTime(FISHING_CATCH_DURATION * 1000)
    expect(get(fishingCatches).size).toBe(0)
  })

  it('cancels a prior expiry when a new cast lands another fish', () => {
    upsertBobber(ID, { x: 1, y: 2, z: 3 })
    landFishingCatch(ID, fish)
    vi.advanceTimersByTime(2000)
    upsertBobber(ID, { x: 4, y: 2, z: 3 })
    expect(get(fishingCatches).size).toBe(0)
    landFishingCatch(ID, fish)
    vi.advanceTimersByTime(2000)
    expect(get(fishingCatches).has(ID)).toBe(true)
    removeFishingCatch(ID)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('clears catches and timers when the angler leaves or the game resets', () => {
    for (const id of [ID, ID + 1]) {
      upsertBobber(id, { x: 1, y: 2, z: 3 })
      landFishingCatch(id, fish)
    }
    removeBobber(ID)
    expect(get(fishingCatches).has(ID)).toBe(false)
    resetFishingStore()
    expect(get(fishingCatches).size).toBe(0)
    expect(vi.getTimerCount()).toBe(0)
  })
})

function fight(overrides: Partial<FightStatus> = {}): FightStatus {
  return {
    fishState: 'running',
    tension: 50,
    stamina: 80,
    trophy: false,
    ...overrides,
  }
}

describe('myFishing transitions', () => {
  beforeEach(() => {
    resetFishingStore()
  })

  it('starts idle', () => {
    expect(get(myFishing)).toEqual({ phase: 'idle' })
  })

  it('the first fight beat after the hook opens the fight phase', () => {
    myFishing.set({ phase: 'bite' })

    applyFightUpdate('running', 20, 100)

    expect(get(myFishing)).toEqual({
      phase: 'fight',
      fight: { fishState: 'running', tension: 20, stamina: 100, trophy: false },
    })
  })

  it('keeps the pre-rolled trophy flag through exhaustion', () => {
    myFishing.set({ phase: 'bite' })
    applyFightUpdate('running', 30, 100, true)
    expect(get(myFishing)).toEqual({
      phase: 'fight',
      fight: { fishState: 'running', tension: 30, stamina: 100, trophy: true },
    })
    applyFightUpdate('exhausted', 50, 0, true)
    expect(get(myFishing)).toEqual({
      phase: 'fight',
      fight: { fishState: 'exhausted', tension: 50, stamina: 0, trophy: true },
    })
  })

  it('later beats update the readout', () => {
    myFishing.set({ phase: 'fight', fight: fight() })

    applyFightUpdate('exhausted', 30, 0)

    expect(get(myFishing)).toEqual({
      phase: 'fight',
      fight: { fishState: 'exhausted', tension: 30, stamina: 0, trophy: false },
    })
  })

  it('a beat racing the end must not resurrect a dead fight', () => {
    for (const phase of ['idle', 'casting'] as const) {
      myFishing.set({ phase })
      applyFightUpdate('running', 50, 50)
      expect(get(myFishing)).toEqual({ phase })
    }
  })
})

describe('fishingBobbers', () => {
  beforeEach(() => {
    resetFishingStore()
  })

  it('upserts a bobber without a bite, landing immediately by default', () => {
    upsertBobber(ID, { x: 1, y: 0, z: 2 })

    expect(get(fishingBobbers).get(ID)).toEqual({
      position: { x: 1, y: 0, z: 2 },
      landsInMs: 0,
      bite: false,
    })
  })

  it('a cast carries its flight time so the float lands late', () => {
    upsertBobber(ID, { x: 1, y: 0, z: 2 }, 1400)

    expect(get(fishingBobbers).get(ID)?.landsInMs).toBe(1400)
  })

  it('re-casting resets a previous bite', () => {
    upsertBobber(ID, { x: 1, y: 0, z: 2 })
    markBobberBite(ID)
    upsertBobber(ID, { x: 3, y: 0, z: 4 })

    expect(get(fishingBobbers).get(ID)).toEqual({
      position: { x: 3, y: 0, z: 4 },
      landsInMs: 0,
      bite: false,
    })
  })

  it('marks a bite only for an existing bobber', () => {
    upsertBobber(ID, { x: 1, y: 0, z: 2 })

    markBobberBite(ID)
    markBobberBite(99)

    const map = get(fishingBobbers)
    expect(map.get(ID)?.bite).toBe(true)
    expect(map.has(99)).toBe(false)
  })

  it('a bite for an unknown player does not touch the map — no spurious rerender', () => {
    upsertBobber(ID, { x: 1, y: 0, z: 2 })
    const before = get(fishingBobbers)

    markBobberBite(99)

    expect(get(fishingBobbers)).toBe(before)
  })

  it('a fight beat moves the bobber, clears the bite, and carries the readout', () => {
    upsertBobber(ID, { x: 1, y: 0, z: 2 })
    markBobberBite(ID)

    updateBobberFight(ID, { x: 3, y: 0, z: 5 }, 'running', 70)

    expect(get(fishingBobbers).get(ID)).toEqual({
      position: { x: 3, y: 0, z: 5 },
      landsInMs: 0,
      bite: false,
      fight: { fishState: 'running', stamina: 70, stance: 'hold' },
    })
  })

  it('a fight beat for an unknown player does not touch the map', () => {
    const before = get(fishingBobbers)

    updateBobberFight(99, { x: 3, y: 0, z: 5 }, 'resting', 50)

    expect(get(fishingBobbers)).toBe(before)
  })

  it('removes a bobber; removing an absent one keeps the same map', () => {
    upsertBobber(ID, { x: 1, y: 0, z: 2 })

    removeBobber(ID)
    expect(get(fishingBobbers).size).toBe(0)

    const before = get(fishingBobbers)
    removeBobber(ID)
    expect(get(fishingBobbers)).toBe(before)
  })

  it('reset clears the phase and every bobber', () => {
    myFishing.set({ phase: 'fight', fight: fight() })
    upsertBobber(ID, { x: 1, y: 0, z: 2 })

    resetFishingStore()

    expect(get(myFishing)).toEqual({ phase: 'idle' })
    expect(get(fishingBobbers).size).toBe(0)
  })

  it('animates local input immediately without an older beat overriding it', () => {
    myFishing.set({ phase: 'bite' })
    setLocalFishingAction('reel')
    expect(fishingReelStance()).toBeNull()
    applyFightUpdate('running', 20, 100)
    expect(fishingReelStance()).toBe('hold')
    setLocalFishingAction('reel')
    applyFightUpdate('running', 25, 90)
    expect(fishingReelStance()).toBe('reel')
    setLocalFishingAction('giveline')
    expect(fishingReelStance()).toBe('giveline')
    setLocalFishingAction('hold')
    expect(fishingReelStance()).toBe('hold')
    myFishing.set({ phase: 'idle' })
    expect(fishingReelStance()).toBeNull()
    myFishing.set({ phase: 'bite' })
    applyFightUpdate('running', 20, 100)
    expect(fishingReelStance()).toBe('hold')
  })

  it('follows remote stance changes and clears them with the bobber', () => {
    upsertBobber(ID, { x: 1, y: 0, z: 2 })
    for (const stance of ['reel', 'giveline', 'hold'] as const) {
      updateBobberFight(ID, { x: 1, y: 0, z: 2 }, 'running', 70, stance)
      expect(fishingReelStance(ID)).toBe(stance)
    }
    removeBobber(ID)
    expect(fishingReelStance(ID)).toBeNull()
    updateBobberFight(ID, { x: 1, y: 0, z: 2 }, 'running', 70, 'reel')
    expect(fishingReelStance(ID)).toBeNull()
  })
})
