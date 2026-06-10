/** Currency is stored as a single integer in the smallest unit (copper).
 *  Gold/silver/copper is purely a display format: 1g = 100s = 10,000c. */
const COPPER_PER_SILVER = 100
const COPPER_PER_GOLD = 10_000

export function formatGold(copper: number): string {
  const negative = copper < 0
  let remaining = Math.abs(Math.trunc(copper))

  const gold = Math.trunc(remaining / COPPER_PER_GOLD)
  remaining %= COPPER_PER_GOLD
  const silver = Math.trunc(remaining / COPPER_PER_SILVER)
  const copperPart = remaining % COPPER_PER_SILVER

  const parts: string[] = []
  if (gold > 0) parts.push(`${gold}g`)
  if (silver > 0) parts.push(`${silver}s`)
  if (copperPart > 0 || parts.length === 0) parts.push(`${copperPart}c`)

  return (negative ? '-' : '') + parts.join(' ')
}
