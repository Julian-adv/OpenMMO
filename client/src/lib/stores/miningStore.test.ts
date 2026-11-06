import { get } from 'svelte/store'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { MINING_SWING_MS } from '../data/combatTiming'
import {
  endMining,
  miningPlayers,
  noteMiningSwing,
  setMining,
} from './miningStore'

describe('mining store', () => {
  afterEach(() => vi.useRealTimers())

  it('keeps the pickaxe out until the swing under way lands', () => {
    vi.useFakeTimers()
    setMining(7, true)
    noteMiningSwing(7, Date.now())
    vi.advanceTimersByTime(500)
    endMining(7)
    expect(get(miningPlayers).has(7)).toBe(true)
    vi.advanceTimersByTime(MINING_SWING_MS - 500)
    expect(get(miningPlayers).has(7)).toBe(false)
  })

  it('puts it away at once between swings, and a new cast keeps it', () => {
    vi.useFakeTimers()
    setMining(8, true)
    endMining(8)
    expect(get(miningPlayers).has(8)).toBe(false)
    setMining(8, true)
    noteMiningSwing(8, Date.now())
    endMining(8)
    setMining(8, true)
    vi.advanceTimersByTime(MINING_SWING_MS)
    expect(get(miningPlayers).has(8)).toBe(true)
  })
})
