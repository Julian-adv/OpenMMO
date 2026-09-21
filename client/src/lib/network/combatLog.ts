import { translate, type MessageKey } from '../i18n'

export function attackLog(
  roll: number,
  hit: boolean,
  damage: number,
  strike?: number | null
) {
  const result = hit
    ? translate('combat.hit', { roll, damage })
    : translate('combat.miss', { roll })
  return strike == null
    ? result
    : translate('combat.doubleSlash', { strike, result })
}

export function daggerSkippedLog(strike: number, reason: string) {
  const reasons: Record<string, MessageKey> = {
    target_defeated: 'combat.target_defeated',
    out_of_range: 'combat.out_of_range',
    interrupted: 'combat.interrupted',
    weapon_changed: 'combat.weapon_changed',
    invalid_target: 'combat.invalid_target',
  }
  return translate('combat.skipped', {
    strike,
    reason: reasons[reason] ? translate(reasons[reason]) : reason,
  })
}
