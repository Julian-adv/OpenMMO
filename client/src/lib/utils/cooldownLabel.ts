/** Tenths under a second, whole seconds above. */
export function cooldownLabel(remainingMs: number) {
  return remainingMs <= 1000
    ? (Math.ceil(remainingMs / 100) / 10).toFixed(1)
    : String(Math.ceil(remainingMs / 1000))
}
