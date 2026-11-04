import { get, writable } from 'svelte/store'

export const TELEPORT_VANISH_MS = 300
export const TELEPORT_REVEAL_MS = 180
export const TELEPORT_DEPARTURE_MS = 520
export const TELEPORT_ARRIVAL_MS = 650

export type TeleportEffect = {
  playerId: number
  position: { x: number; y: number; z: number }
  floorLevel: number
  phase: 'Departing' | 'Arriving' | 'Cancelled'
  startedAt: number
}

export const teleportHiddenPlayers = writable<ReadonlySet<number>>(new Set())
export const localTeleportActive = writable(false)
const timers = new Map<number, ReturnType<typeof setTimeout>[]>()
const arrivals = new Map<
  number,
  { event: Omit<TeleportEffect, 'startedAt'>; local: boolean }
>()
let pending: TeleportEffect[] = []

/** Sends at once; the server holds the teleport for the departure and
 * cancels it if the player moves. */
export function beginLocalTeleport(
  event: Omit<TeleportEffect, 'startedAt' | 'phase'>,
  request: () => boolean
) {
  if (get(localTeleportActive) || !request()) return false
  playTeleportEffect({ ...event, phase: 'Departing' }, true)
  return true
}

function setHidden(playerId: number, hidden: boolean) {
  teleportHiddenPlayers.update((players) => {
    const next = new Set(players)
    if (hidden) next.add(playerId)
    else next.delete(playerId)
    return next
  })
}

function clearTimers(playerId: number) {
  timers.get(playerId)?.forEach(clearTimeout)
  timers.delete(playerId)
}

export function playTeleportEffect(
  event: Omit<TeleportEffect, 'startedAt'>,
  local: boolean
) {
  clearTimers(event.playerId)
  arrivals.delete(event.playerId)
  const release = () => {
    clearTimers(event.playerId)
    arrivals.delete(event.playerId)
    setHidden(event.playerId, false)
    if (local) localTeleportActive.set(false)
  }
  if (event.phase === 'Cancelled') {
    pending.push({ ...event, startedAt: Date.now() })
    release()
    return
  }
  if (local) localTeleportActive.set(true)
  setHidden(event.playerId, event.phase === 'Arriving')
  if (event.phase === 'Arriving') {
    arrivals.set(event.playerId, { event, local })
    timers.set(event.playerId, [setTimeout(release, 3000)])
    return
  }
  pending.push({ ...event, startedAt: Date.now() })
  timers.set(event.playerId, [
    setTimeout(() => setHidden(event.playerId, true), TELEPORT_VANISH_MS),
    setTimeout(release, 3000),
  ])
}

export function finishTeleportArrival(playerId: number) {
  const arrival = arrivals.get(playerId)
  if (!arrival) return
  arrivals.delete(playerId)
  clearTimers(playerId)
  setHidden(playerId, true)
  pending.push({ ...arrival.event, startedAt: Date.now() })
  timers.set(playerId, [
    setTimeout(() => {
      clearTimers(playerId)
      setHidden(playerId, false)
      if (arrival.local) localTeleportActive.set(false)
    }, TELEPORT_REVEAL_MS),
  ])
}

export function takeTeleportEffects() {
  const events = pending
  pending = []
  return events
}

export function resetTeleportEffects() {
  for (const playerId of timers.keys()) clearTimers(playerId)
  pending = []
  arrivals.clear()
  teleportHiddenPlayers.set(new Set())
  localTeleportActive.set(false)
}
