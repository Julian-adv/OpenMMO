import { writable } from 'svelte/store'
import { MINING_SWING_MS } from '../data/combatTiming'

export const PICKAXE_ITEM = 'miner_pickaxe'
export const PICKAXE_ITEMS: readonly string[] = [PICKAXE_ITEM, 'foreman_pickaxe']

/** Players mid-mining cast; they hold a pickaxe until it ends. */
export const miningPlayers = writable<ReadonlySet<number>>(new Set())

const swingStarts = new Map<number, number>()
const putAways = new Map<number, ReturnType<typeof setTimeout>>()

function cancelPutAway(playerId: number) {
  clearTimeout(putAways.get(playerId))
  putAways.delete(playerId)
}

export function setMining(playerId: number, mining: boolean) {
  cancelPutAway(playerId)
  if (!mining) swingStarts.delete(playerId)
  miningPlayers.update((ids) => {
    if (ids.has(playerId) === mining) return ids
    const next = new Set(ids)
    if (mining) next.add(playerId)
    else next.delete(playerId)
    return next
  })
}

export function noteMiningSwing(playerId: number) {
  swingStarts.set(playerId, Date.now())
}

/** The cast ended: put the pickaxe away once the swing under way lands.
 *  Returns the ms that swing has left. */
export function endMining(playerId: number) {
  const at = swingStarts.get(playerId)
  const left =
    at === undefined ? 0 : Math.max(0, MINING_SWING_MS - (Date.now() - at))
  if (left <= 0) {
    setMining(playerId, false)
    return 0
  }
  cancelPutAway(playerId)
  putAways.set(
    playerId,
    setTimeout(() => setMining(playerId, false), left)
  )
  return left
}
