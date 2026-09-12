export function attackLog(
  roll: number,
  hit: boolean,
  damage: number,
  strike?: number | null
) {
  const prefix = strike == null ? '' : `Double Slash ${strike}/2 — `
  return (
    prefix +
    (hit
      ? `rolled ${roll}: HIT for ${damage} damage!`
      : `rolled ${roll}: MISSED!`)
  )
}

export function daggerSkippedLog(strike: number, reason: string) {
  const reasons: Record<string, string> = {
    target_defeated: 'target defeated',
    out_of_range: 'target out of reach',
    interrupted: 'attack interrupted',
    weapon_changed: 'weapon changed',
    invalid_target: 'target unavailable',
  }
  return `Double Slash ${strike}/2: SKIPPED — ${reasons[reason] ?? reason}.`
}
