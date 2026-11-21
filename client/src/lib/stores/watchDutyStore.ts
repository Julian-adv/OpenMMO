import { writable } from 'svelte/store'

export interface WatchDuty {
  stage: 'waiting' | 'on_watch' | 'done'
  gear: 'unissued' | 'issued'
  calls: number
  answered: number
  shovel: boolean
}

/** The guard captain's terms for a watch, awaiting yes or no. */
export interface WatchOffer {
  npcPlayerId: number
  sergeant: string
  departsMinute: number
  wage: number
}

/** The watch house gear rack as the recruit standing at it sees it. */
export interface WatchRack {
  objectId: number
  /** Item ids hanging there to take. */
  onRack: string[]
  /** Item ids drawn and not yet back. */
  owed: string[]
  returnable: boolean
}

/** The player's standing as a day recruit of the guard; null off the roll. */
export const watchDuty = writable<WatchDuty | null>(null)
export const watchOffer = writable<WatchOffer | null>(null)
export const watchRack = writable<WatchRack | null>(null)

/** Whether the recruit has gear to draw from the rack or to put back. */
export function rackDue(duty: WatchDuty | null): boolean {
  return !!duty && (duty.gear === 'issued') === (duty.stage === 'done')
}
