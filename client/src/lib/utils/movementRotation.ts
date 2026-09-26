import { angleDelta } from './horseMovement'

const TURN_DURATION = 0.12

export class MovementRotation {
  private rotation: number | null = null
  private from = 0
  private target = 0
  private elapsed = TURN_DURATION

  update(target: number, deltaTime: number, smooth: boolean): number {
    if (this.rotation === null || !smooth) {
      this.rotation = this.from = this.target = target
      this.elapsed = TURN_DURATION
      return target
    }
    if (Math.abs(angleDelta(this.target, target)) > 1e-6) {
      this.from = this.rotation
      this.target = target
      this.elapsed = 0
    }
    this.elapsed = Math.min(
      this.elapsed + Math.max(0, deltaTime),
      TURN_DURATION
    )
    const fraction = 1 - (1 - this.elapsed / TURN_DURATION) ** 2
    this.rotation =
      this.elapsed === TURN_DURATION
        ? this.target
        : this.from + angleDelta(this.from, this.target) * fraction
    return this.rotation
  }
}
