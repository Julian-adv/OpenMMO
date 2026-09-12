import { beforeEach, describe, expect, it } from 'vitest'
import {
  queueEnchantSuccess,
  takeEnchantSuccesses,
} from './enchantSuccessStore'

beforeEach(() => {
  takeEnchantSuccesses()
})
describe('enchant event queue', () => {
  it('delivers an event only once', () => {
    queueEnchantSuccess(42, true)
    expect(takeEnchantSuccesses()).toMatchObject([
      { playerId: 42, weapon: true },
    ])
    expect(takeEnchantSuccesses()).toEqual([])
  })
  it('retains every player while coalescing repeated events from one player', () => {
    for (let i = 0; i < 100; i++) queueEnchantSuccess(i, false)
    queueEnchantSuccess(99, true)
    const events = takeEnchantSuccesses()
    expect(events).toHaveLength(100)
    expect(events.filter((event) => event.playerId === 99)).toHaveLength(1)
    expect(events.at(-1)?.weapon).toBe(true)
  })
})
