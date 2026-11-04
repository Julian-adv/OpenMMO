import { writable } from 'svelte/store'

export const PICKAXE_ITEM = 'miner_pickaxe'

/** Players mid-mining cast; they hold a pickaxe until it ends. */
export const miningPlayers = writable<ReadonlySet<number>>(new Set())

export function setMining(playerId: number, mining: boolean) {
  miningPlayers.update((ids) => {
    if (ids.has(playerId) === mining) return ids
    const next = new Set(ids)
    if (mining) next.add(playerId)
    else next.delete(playerId)
    return next
  })
}
