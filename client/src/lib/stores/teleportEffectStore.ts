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
const departures = new Map<
  number,
  {
    timer: ReturnType<typeof setTimeout>
    updates: (() => void)[]
  }
>()
let localRequest: {
  event: Omit<TeleportEffect, 'startedAt'>
  timer: ReturnType<typeof setTimeout>
} | null = null

export function beginLocalTeleport(
  event: Omit<TeleportEffect, 'startedAt' | 'phase'>,
  request: () => boolean
) {
  if (get(localTeleportActive)) return false
  const departure = { ...event, phase: 'Departing' as const }
  playTeleportEffect(departure, true)
  localRequest = {
    event: departure,
    timer: setTimeout(() => {
      localRequest = null
      if (!request())
        playTeleportEffect({ ...departure, phase: 'Cancelled' }, true)
    }, TELEPORT_DEPARTURE_MS),
  }
  return true
}

export function cancelLocalTeleportRequest() {
  if (!localRequest) return
  playTeleportEffect({ ...localRequest.event, phase: 'Cancelled' }, true)
}

export function deferRemoteTeleportUpdate(
  playerId: number,
  update: () => void
) {
  const departure = departures.get(playerId)
  if (!departure) return false
  departure.updates.push(update)
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
  if (event.phase === 'Arriving' && departures.has(event.playerId)) {
    arrivals.set(event.playerId, { event, local })
    return
  }
  if (local && event.phase !== 'Departing' && localRequest) {
    clearTimeout(localRequest.timer)
    localRequest = null
  }
  clearTimers(event.playerId)
  arrivals.delete(event.playerId)
  const release = () => {
    clearTimers(event.playerId)
    arrivals.delete(event.playerId)
    setHidden(event.playerId, false)
    if (local) localTeleportActive.set(false)
  }
  if (event.phase === 'Cancelled') {
    const departure = departures.get(event.playerId)
    if (departure) clearTimeout(departure.timer)
    departures.delete(event.playerId)
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
  if (!local) {
    const previous = departures.get(event.playerId)
    if (previous) clearTimeout(previous.timer)
    const updates = previous?.updates ?? []
    departures.set(event.playerId, {
      updates,
      timer: setTimeout(() => {
        departures.delete(event.playerId)
        for (const update of updates) update()
      }, TELEPORT_DEPARTURE_MS),
    })
  }
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
  if (localRequest) clearTimeout(localRequest.timer)
  localRequest = null
  for (const departure of departures.values()) clearTimeout(departure.timer)
  departures.clear()
  for (const playerId of timers.keys()) clearTimers(playerId)
  pending = []
  arrivals.clear()
  teleportHiddenPlayers.set(new Set())
  localTeleportActive.set(false)
}
