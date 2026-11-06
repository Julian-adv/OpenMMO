import { writable } from 'svelte/store'
import { MINING_SWING_MS } from '../data/combatTiming'

export const PICKAXE_ITEM = 'miner_pickaxe'

/** Players mid-mining cast; they hold a pickaxe until it ends. */
export const miningPlayers = writable<ReadonlySet<number>>(new Set())

const swingStarts = new Map<number, number>()
const putAways = new Map<number, ReturnType<typeof setTimeout>>()

export function setMining(playerId: number, mining: boolean) {
  clearTimeout(putAways.get(playerId))
  putAways.delete(playerId)
  if (!mining) swingStarts.delete(playerId)
  miningPlayers.update((ids) => {
    if (ids.has(playerId) === mining) return ids
    const next = new Set(ids)
    if (mining) next.add(playerId)
    else next.delete(playerId)
    return next
  })
}

export function noteMiningSwing(playerId: number, now = Date.now()) {
  swingStarts.set(playerId, now)
}

/** Time left in the pickaxe swing under way, if any. */
export function miningSwingLeftMs(playerId: number, now = Date.now()) {
  const at = swingStarts.get(playerId)
  return at === undefined ? 0 : Math.max(0, MINING_SWING_MS - (now - at))
}

/** The cast ended: put the pickaxe away once the swing under way lands. */
export function endMining(playerId: number, now = Date.now()) {
  const left = miningSwingLeftMs(playerId, now)
  if (left <= 0) return setMining(playerId, false)
  clearTimeout(putAways.get(playerId))
  putAways.set(
    playerId,
    setTimeout(() => setMining(playerId, false), left)
  )
}
