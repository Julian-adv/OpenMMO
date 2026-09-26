import { smoothstep } from '../terrain/terrain-constants'
import { DAYS_PER_YEAR, HOURS_PER_DAY } from './celestialSimulation'

export interface FoliageSeason {
  /** 0 green .. 1 straw-yellow grass. */
  grassDry: number
  /** 0 green .. 1 autumn-coloured leaves. */
  leafTurn: number
  /** 0 full canopy .. 1 bare branches. */
  leafFall: number
}

/** Days since winter began on 12/30, as `winter_day` in
 *  shared/src/weather/seasonal.rs, so foliage follows the snow season. */
export function winterDay(gameMinutes: number): number {
  const d = gameMinutes / (HOURS_PER_DAY * 60) + 1
  return ((d % DAYS_PER_YEAR) + DAYS_PER_YEAR) % DAYS_PER_YEAR
}

/** Leaves turn through October and fall in November; grass browns in late
 *  autumn. Both come back green in April. */
export function foliageSeason(day: number): FoliageSeason {
  if (day >= DAYS_PER_YEAR / 2) {
    return {
      grassDry: smoothstep(290, 340, day),
      leafTurn: smoothstep(270, 310, day),
      leafFall: smoothstep(315, 350, day),
    }
  }
  return {
    grassDry: 1 - smoothstep(75, 115, day),
    leafTurn: 0,
    leafFall: 1 - smoothstep(85, 125, day),
  }
}
