/** Real seconds between model samples; cover moves at most a few percent in
 *  that time. */
export const SNOW_SAMPLE_SECONDS = 2
const EASE_SECONDS = 3
/** Admin overrides have no history, so they build and melt locally at about
 *  the shared model's midwinter pace (90 and 1,000 game minutes). */
const OVERRIDE_FILL_SECONDS = 675
const OVERRIDE_MELT_SECONDS = 7500

export class SnowCoverTracker {
  private target = 0
  private overrideCover = 0
  private sinceSample = Infinity
  private snap = true
  value = 0

  /** `sample` returns the shared model's cover at the player, or null when
   *  it cannot be evaluated yet. `overrideSnow` is the forced snowfall. */
  update(
    seconds: number,
    sample: () => number | null,
    overrideSnow: number | null
  ): number {
    const dt = Math.max(0, seconds)
    this.sinceSample += dt
    if (this.sinceSample >= SNOW_SAMPLE_SECONDS) {
      this.sinceSample = 0
      const next = sample()
      if (next !== null) {
        this.target = next
        if (this.snap) this.value = next
        this.snap = false
      }
    }
    const snowing = overrideSnow ?? 0
    this.overrideCover =
      snowing > 0.02
        ? Math.min(
            1,
            this.overrideCover + (snowing * dt) / OVERRIDE_FILL_SECONDS
          )
        : Math.max(0, this.overrideCover - dt / OVERRIDE_MELT_SECONDS)
    const goal = Math.max(this.target, this.overrideCover)
    this.value += (goal - this.value) * (1 - Math.exp(-dt / EASE_SECONDS))
    return this.value
  }
}
