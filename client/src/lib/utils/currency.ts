/** Currency is stored as a single integer in the smallest unit (copper).
 *  Gold/silver/copper is purely a display format: 1g = 100s = 10,000c. */
const COPPER_PER_SILVER = 100
const COPPER_PER_GOLD = 10_000

export interface GoldParts {
  negative: boolean
  gold: number
  silver: number
  copper: number
}

export function splitGold(copper: number): GoldParts {
  const negative = copper < 0
  let remaining = Math.abs(Math.trunc(copper))

  const gold = Math.trunc(remaining / COPPER_PER_GOLD)
  remaining %= COPPER_PER_GOLD
  const silver = Math.trunc(remaining / COPPER_PER_SILVER)

  return { negative, gold, silver, copper: remaining % COPPER_PER_SILVER }
}
