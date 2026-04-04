const MAX_SAFE_XP = Number.MAX_SAFE_INTEGER

export function xpForLevel(level: number): number {
  if (level <= 1) return 0
  const shift = level - 2
  if (shift >= 53) return MAX_SAFE_XP
  const threshold = 20 * 2 ** shift
  if (!Number.isFinite(threshold)) return MAX_SAFE_XP
  return Math.min(MAX_SAFE_XP, Math.floor(threshold))
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}
