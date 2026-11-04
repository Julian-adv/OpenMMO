import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import {
  finishTeleportArrival,
  localTeleportActive,
  playTeleportEffect,
  resetTeleportEffects,
  takeTeleportEffects,
  teleportHiddenPlayers,
  TELEPORT_REVEAL_MS,
  TELEPORT_VANISH_MS,
  type TeleportEffect,
  beginLocalTeleport,
  TELEPORT_DEPARTURE_MS,
} from './teleportEffectStore'

const event = (phase: TeleportEffect['phase'], playerId = 1) => ({
  playerId,
  phase,
  position: { x: 10, y: 5, z: 20 },
  floorLevel: 0,
})

beforeEach(() => vi.useFakeTimers())
afterEach(() => {
  resetTeleportEffects()
  vi.useRealTimers()
})

describe('teleport presentation', () => {
  it('sends one request at once and plays the departure while the server holds it', () => {
    const request = vi.fn(() => true)
    expect(beginLocalTeleport(event('Departing'), request)).toBe(true)
    expect(beginLocalTeleport(event('Departing'), request)).toBe(false)
    expect(request).toHaveBeenCalledTimes(1)
    vi.advanceTimersByTime(TELEPORT_DEPARTURE_MS)
    expect(get(teleportHiddenPlayers).has(1)).toBe(true)
    expect(get(localTeleportActive)).toBe(true)
    playTeleportEffect(event('Arriving'), true)
    finishTeleportArrival(1)
    vi.advanceTimersByTime(TELEPORT_REVEAL_MS)
    expect(get(localTeleportActive)).toBe(false)
    expect(takeTeleportEffects().map(({ phase }) => phase)).toEqual([
      'Departing',
      'Arriving',
    ])
  })

  it('plays nothing when the request cannot be sent and restores a server cancel', () => {
    expect(beginLocalTeleport(event('Departing'), () => false)).toBe(false)
    expect(get(localTeleportActive)).toBe(false)
    expect(takeTeleportEffects()).toEqual([])
    beginLocalTeleport(event('Departing'), () => true)
    vi.advanceTimersByTime(TELEPORT_DEPARTURE_MS)
    playTeleportEffect(event('Cancelled'), true)
    expect(get(localTeleportActive)).toBe(false)
    expect(get(teleportHiddenPlayers).size).toBe(0)
  })

  it('clears the local departure and remote effects on disconnect', () => {
    beginLocalTeleport(event('Departing'), () => true)
    playTeleportEffect(event('Departing', 2), false)
    resetTeleportEffects()
    vi.runAllTimers()
    expect(get(localTeleportActive)).toBe(false)
    expect(get(teleportHiddenPlayers).size).toBe(0)
  })

  it('vanishes in the departing beam and reveals when the arriving beam lands', () => {
    playTeleportEffect(event('Departing'), true)
    expect(get(localTeleportActive)).toBe(true)
    expect(get(teleportHiddenPlayers).has(1)).toBe(false)
    vi.advanceTimersByTime(TELEPORT_VANISH_MS)
    expect(get(teleportHiddenPlayers).has(1)).toBe(true)
    playTeleportEffect(event('Arriving'), true)
    finishTeleportArrival(1)
    vi.advanceTimersByTime(TELEPORT_REVEAL_MS - 1)
    expect(get(teleportHiddenPlayers).has(1)).toBe(true)
    vi.advanceTimersByTime(1)
    expect(get(teleportHiddenPlayers).has(1)).toBe(false)
    expect(get(localTeleportActive)).toBe(false)
    expect(takeTeleportEffects().map(({ phase }) => phase)).toEqual([
      'Departing',
      'Arriving',
    ])
  })

  it('waits for the destination position before starting the arrival clock', () => {
    playTeleportEffect(event('Arriving'), true)
    vi.advanceTimersByTime(800)
    expect(get(teleportHiddenPlayers).has(1)).toBe(true)
    expect(takeTeleportEffects()).toHaveLength(0)
    finishTeleportArrival(1)
    vi.advanceTimersByTime(100)
    finishTeleportArrival(1)
    vi.advanceTimersByTime(TELEPORT_REVEAL_MS - 100)
    expect(get(teleportHiddenPlayers).has(1)).toBe(false)
    expect(takeTeleportEffects()).toHaveLength(1)
  })

  it('does not let an old departure timer hide an already arrived player', () => {
    playTeleportEffect(event('Departing'), true)
    vi.advanceTimersByTime(50)
    playTeleportEffect(event('Arriving'), true)
    finishTeleportArrival(1)
    vi.advanceTimersByTime(TELEPORT_VANISH_MS)
    expect(get(teleportHiddenPlayers).size).toBe(0)
    expect(get(localTeleportActive)).toBe(false)
  })

  it('keeps simultaneous remote effects independent of local controls', () => {
    playTeleportEffect(event('Departing'), true)
    playTeleportEffect(event('Arriving', 2), false)
    finishTeleportArrival(2)
    vi.advanceTimersByTime(TELEPORT_REVEAL_MS)
    expect(get(localTeleportActive)).toBe(true)
    expect(get(teleportHiddenPlayers).has(2)).toBe(false)
    vi.advanceTimersByTime(TELEPORT_VANISH_MS)
    expect(get(teleportHiddenPlayers).has(1)).toBe(true)
  })

  it('restores visibility and controls on cancellation or a missing arrival', () => {
    playTeleportEffect(event('Departing'), true)
    vi.advanceTimersByTime(TELEPORT_VANISH_MS)
    playTeleportEffect(event('Cancelled'), true)
    expect(get(teleportHiddenPlayers).size).toBe(0)
    expect(get(localTeleportActive)).toBe(false)
    playTeleportEffect(event('Departing'), true)
    vi.advanceTimersByTime(3000)
    expect(get(teleportHiddenPlayers).size).toBe(0)
    expect(get(localTeleportActive)).toBe(false)
  })

  it('discards timers and pending arrival on reconnect', () => {
    playTeleportEffect(event('Departing'), true)
    playTeleportEffect(event('Arriving', 2), false)
    resetTeleportEffects()
    finishTeleportArrival(2)
    vi.runAllTimers()
    expect(get(teleportHiddenPlayers).size).toBe(0)
    expect(get(localTeleportActive)).toBe(false)
    expect(takeTeleportEffects()).toHaveLength(0)
    expect(vi.getTimerCount()).toBe(0)
  })
})
