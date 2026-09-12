export const PREWARM_ENCHANT_EFFECTS = 8
export type EnchantSuccess = {
  playerId: number
  weapon: boolean
  startedAt: number
}
let pending: EnchantSuccess[] = []

export function queueEnchantSuccess(playerId: number, weapon: boolean) {
  pending = pending.filter((event) => event.playerId !== playerId)
  pending.push({ playerId, weapon, startedAt: Date.now() })
}

export function takeEnchantSuccesses(): EnchantSuccess[] {
  const events = pending
  pending = []
  return events
}
