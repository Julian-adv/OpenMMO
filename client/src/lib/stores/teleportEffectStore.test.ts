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
  cancelLocalTeleportRequest,
  deferRemoteTeleportUpdate,
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
  it('sends one request only after the local departure finishes', () => {
    const request = vi.fn(() => true)
    expect(beginLocalTeleport(event('Departing'), request)).toBe(true)
    expect(beginLocalTeleport(event('Departing'), request)).toBe(false)
    vi.advanceTimersByTime(TELEPORT_DEPARTURE_MS - 1)
    expect(request).not.toHaveBeenCalled()
    expect(get(teleportHiddenPlayers).has(1)).toBe(true)
    vi.advanceTimersByTime(1)
    expect(request).toHaveBeenCalledTimes(1)
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

  it('never sends a cancelled request and restores a failed departure', () => {
    const request = vi.fn(() => true)
    beginLocalTeleport(event('Departing'), request)
    cancelLocalTeleportRequest()
    vi.runAllTimers()
    expect(request).not.toHaveBeenCalled()
    beginLocalTeleport(event('Departing'), () => false)
    vi.advanceTimersByTime(TELEPORT_DEPARTURE_MS)
    expect(get(localTeleportActive)).toBe(false)
    expect(get(teleportHiddenPlayers).size).toBe(0)
  })

  it('keeps a remote departure visible when departure and arrival arrive together', () => {
    const applied: string[] = []
    playTeleportEffect(event('Departing', 2), false)
    playTeleportEffect(event('Arriving', 2), false)
    expect(
      deferRemoteTeleportUpdate(2, () => {
        applied.push('teleported')
        finishTeleportArrival(2)
      })
    ).toBe(true)
    deferRemoteTeleportUpdate(2, () => applied.push('moved'))
    expect(get(teleportHiddenPlayers).has(2)).toBe(false)
    vi.advanceTimersByTime(TELEPORT_VANISH_MS)
    expect(get(teleportHiddenPlayers).has(2)).toBe(true)
    expect(applied).toEqual([])
    vi.advanceTimersByTime(TELEPORT_DEPARTURE_MS - TELEPORT_VANISH_MS)
    expect(applied).toEqual(['teleported', 'moved'])
    expect(get(teleportHiddenPlayers).has(2)).toBe(true)
    vi.advanceTimersByTime(TELEPORT_REVEAL_MS)
    expect(get(teleportHiddenPlayers).has(2)).toBe(false)
    expect(deferRemoteTeleportUpdate(2, () => {})).toBe(false)
  })

  it('clears both the unsent request and deferred remote updates on disconnect', () => {
    const request = vi.fn(() => true)
    const update = vi.fn()
    beginLocalTeleport(event('Departing'), request)
    playTeleportEffect(event('Departing', 2), false)
    deferRemoteTeleportUpdate(2, update)
    resetTeleportEffects()
    vi.runAllTimers()
    expect(request).not.toHaveBeenCalled()
    expect(update).not.toHaveBeenCalled()
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
