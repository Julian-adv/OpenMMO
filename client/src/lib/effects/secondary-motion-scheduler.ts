export const REMOTE_PHYSICS_STEP = 1 / 30
const MAX_GAP = 0.25

export function createSecondaryMotionScheduler(seed = 0) {
  let visible = false
  let elapsed = 0
  let remaining = 0
  const result: { deltaTime: number; reset: boolean; step?: number } = {
    deltaTime: 0,
    reset: false,
  }

  function emit(deltaTime: number, reset: boolean, step?: number) {
    result.deltaTime = deltaTime
    result.reset = reset
    result.step = step
    return result
  }

  return {
    next(dt: number, onScreen: boolean, local: boolean) {
      if (!onScreen) {
        visible = false
        elapsed = 0
        return null
      }
      const step = local ? undefined : REMOTE_PHYSICS_STEP
      if (!visible) {
        visible = true
        remaining = REMOTE_PHYSICS_STEP * (1 + (seed % 2) * 0.5)
        return emit(dt, true, step)
      }
      if (local) {
        elapsed = 0
        remaining = REMOTE_PHYSICS_STEP
        return emit(dt, dt > MAX_GAP)
      }
      elapsed += dt
      remaining -= dt
      if (remaining > 1e-10) return null
      remaining = REMOTE_PHYSICS_STEP + (remaining % REMOTE_PHYSICS_STEP)
      const deltaTime = elapsed
      elapsed = 0
      return emit(deltaTime, deltaTime > MAX_GAP, step)
    },
  }
}
