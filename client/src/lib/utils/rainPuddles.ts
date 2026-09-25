export const PUDDLE_FILL_SECONDS = 90
export const PUDDLE_DRY_SECONDS = 240
export const PUDDLE_VISIBLE_WETNESS = 0.18
export const PUDDLE_SAMPLE_BUDGET = 8
const MAX_CACHED_POINTS = 256
const HISTORY_STEP = 10

export type RainSampler = (x: number, z: number, secondsAgo: number) => number

interface WetPoint {
  x: number
  z: number
  wetness: number
  rain: number
  lastSeen: number
  nextSample: number
  initialized: boolean
  history: { remaining: number; wetness: number; started: number } | null
}

export function advanceRainWetness(
  wetness: number,
  rain: number,
  seconds: number
): number {
  const intensity = Math.max(0, Math.min(1, rain))
  const rate =
    intensity > 0.02 ? intensity / PUDDLE_FILL_SECONDS : -1 / PUDDLE_DRY_SECONDS
  return Math.max(0, Math.min(1, wetness + rate * Math.max(0, seconds)))
}

export class RainPuddleTracker {
  private points = new Map<string, WetPoint>()
  private active = new Set<WetPoint>()
  private historyQueue: WetPoint[] = []
  private elapsed = 0

  pause(seconds: number) {
    this.elapsed += Math.max(0, seconds)
    this.active.clear()
  }

  update(seconds: number, sampleRain: RainSampler, restoreHistory = true) {
    const dt = Math.max(0, seconds)
    this.elapsed += dt
    if (!restoreHistory) {
      for (const point of this.historyQueue) {
        point.history = null
        point.initialized = true
      }
      this.historyQueue.length = 0
    }
    let samples = 0
    for (const point of this.active) {
      if (point.nextSample <= this.elapsed && samples < PUDDLE_SAMPLE_BUDGET) {
        point.rain = sampleRain(point.x, point.z, 0)
        point.nextSample = this.elapsed + 1
        samples++
      }
      point.wetness = advanceRainWetness(point.wetness, point.rain, dt)
      point.lastSeen = this.elapsed
    }
    while (samples < PUDDLE_SAMPLE_BUDGET && this.historyQueue.length > 0) {
      const point = this.historyQueue.shift()!
      const history = point.history
      if (!history) continue
      if (!this.active.has(point)) {
        point.history = null
        continue
      }
      const age = this.elapsed - history.started
      history.wetness = advanceRainWetness(
        history.wetness,
        sampleRain(
          point.x,
          point.z,
          history.remaining - HISTORY_STEP / 2 + age
        ),
        HISTORY_STEP
      )
      samples++
      history.remaining -= HISTORY_STEP
      if (history.remaining > 0) {
        this.historyQueue.push(point)
      } else {
        point.wetness = advanceRainWetness(history.wetness, point.rain, age)
        point.history = null
        point.initialized = true
      }
    }
    this.active.clear()
    return samples
  }

  sample(x: number, z: number, restoreHistory: boolean) {
    const key = `${x},${z}`
    let point = this.points.get(key)
    if (point && this.elapsed - point.lastSeen > PUDDLE_DRY_SECONDS * 2) {
      point.history = null
      this.points.delete(key)
      point = undefined
    }
    if (!point) {
      point = {
        x,
        z,
        wetness: 0,
        rain: 0,
        lastSeen: this.elapsed,
        nextSample: 0,
        initialized: !restoreHistory,
        history: null,
      }
      this.points.set(key, point)
      if (restoreHistory) this.restore(point)
    } else if (this.elapsed - point.lastSeen > 2) {
      if (restoreHistory) {
        this.restore(point)
      } else {
        point.wetness = advanceRainWetness(
          point.wetness,
          0,
          this.elapsed - point.lastSeen
        )
      }
      point.nextSample = 0
    }
    if (!restoreHistory) {
      point.history = null
      point.initialized = true
    } else if (!point.initialized) {
      this.restore(point)
    }
    point.lastSeen = this.elapsed
    this.active.add(point)
    if (this.points.size > MAX_CACHED_POINTS) {
      for (const [oldKey, oldPoint] of this.points) {
        if (this.active.has(oldPoint)) continue
        oldPoint.history = null
        this.points.delete(oldKey)
        if (this.points.size <= MAX_CACHED_POINTS) break
      }
    }
    return point
  }

  private restore(point: WetPoint) {
    if (point.history) return
    point.initialized = false
    point.history = {
      remaining: PUDDLE_FILL_SECONDS + PUDDLE_DRY_SECONDS,
      wetness: 0,
      started: this.elapsed,
    }
    this.historyQueue.push(point)
  }
}
