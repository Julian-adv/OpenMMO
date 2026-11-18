import { writable } from 'svelte/store'
import { SNOW_CLEAR_LASTS_MIN, type SnowClearing } from '../utils/roadSnowMask'

export const SNOW_DIG_PACK = 'animations/snow_dig.glb'
export const SNOW_DIG_CLIP = 'snow_dig'
/** How long one bout of shovelling lasts; the dig clip runs this long. */
export const SNOW_DIG_MS = 4900
/** Held while shovelling, whatever is in the main hand. */
export const SNOW_SHOVEL_ITEM = 'snow_shovel'

/** Player id → when their latest bout of shovelling was broadcast. */
export const snowDigs = writable(new Map<number, number>())

export function recordSnowDig(playerId: number) {
  const now = Date.now()
  snowDigs.update((digs) => {
    const next = new Map([...digs].filter(([, at]) => now - at < 10000))
    next.set(playerId, now)
    return next
  })
}

/** Road stretches shovelled clear, and a counter that moves when they change. */
export const roadSnow: { stretches: SnowClearing[]; revision: number } = {
  stretches: [],
  revision: 0,
}

function addStretches(reset: boolean, stretches: SnowClearing[]) {
  const newest = stretches.reduce((latest, s) => Math.max(latest, s.at_min), 0)
  const kept = reset
    ? []
    : roadSnow.stretches.filter(
        (stretch) => newest - stretch.at_min < SNOW_CLEAR_LASTS_MIN
      )
  roadSnow.stretches = [...kept, ...stretches]
  roadSnow.revision += 1
}

/** A stretch being shovelled now loses its snow when the bout ends; the list
 *  sent on entry is already clear. */
export function applyRoadSnowCleared(
  reset: boolean,
  stretches: SnowClearing[]
) {
  if (reset) addStretches(true, stretches)
  else setTimeout(() => addStretches(false, stretches), SNOW_DIG_MS)
}
