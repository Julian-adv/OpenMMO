export type LoopProfileSection =
  | 'frameWork'
  | 'cameraOffset'
  | 'playerControl'
  | 'remoteInterpolation'
  | 'currentPlayerAnimation'
  | 'otherPlayerAnimation'
  | 'monsterAnimation'
  | 'monsterLogic'
  | 'cameraUpdate'
  | 'lightUpdate'
  | 'housingUpdate'
  | 'grassUpdate'
  | 'windParticles'
  | 'wetnessPass'
  | 'refractionPass'
  | 'reflectionPass'
  | 'mainRenderCpu'

export const LOOP_PROFILE_SECTIONS: readonly LoopProfileSection[] = [
  'frameWork',
  'cameraOffset',
  'playerControl',
  'remoteInterpolation',
  'currentPlayerAnimation',
  'otherPlayerAnimation',
  'monsterAnimation',
  'monsterLogic',
  'cameraUpdate',
  'lightUpdate',
  'housingUpdate',
  'grassUpdate',
  'windParticles',
  'wetnessPass',
  'refractionPass',
  'reflectionPass',
  'mainRenderCpu',
] as const

interface LoopProfileStats {
  totalMs: number
  maxMs: number
  count: number
}

const PROFILE_WINDOW_MS = 1000
const FRAME_DROP_THRESHOLD_MULTIPLIER = 1.5

export interface LoopProfiler {
  resetWindow: (windowStart: number) => void
  onFrame: (fixedDeltaMs: number, frameTimeMs: number) => void
  record: (section: LoopProfileSection, durationMs: number) => void
  recordCount: (name: string, value: number) => void
  flush: (now: number) => void
}

export function createLoopProfiler(isEnabled: () => boolean): LoopProfiler {
  const statsBySection = new Map<LoopProfileSection, LoopProfileStats>(
    LOOP_PROFILE_SECTIONS.map((section) => [
      section,
      { totalMs: 0, maxMs: 0, count: 0 },
    ])
  )
  const counterStats = new Map<string, LoopProfileStats>()

  let windowStart = 0
  let frameCount = 0
  let frameDropCount = 0
  let rawDeltaTotal = 0
  let rawDeltaMax = 0

  function resetWindow(nextWindowStart: number) {
    windowStart = nextWindowStart
    frameCount = 0
    frameDropCount = 0
    rawDeltaTotal = 0
    rawDeltaMax = 0

    for (const section of LOOP_PROFILE_SECTIONS) {
      const stats = statsBySection.get(section)
      if (!stats) continue
      stats.totalMs = 0
      stats.maxMs = 0
      stats.count = 0
    }
    for (const stats of counterStats.values()) {
      stats.totalMs = 0
      stats.maxMs = 0
      stats.count = 0
    }
  }

  function recordCount(name: string, value: number) {
    if (!isEnabled()) return
    let stats = counterStats.get(name)
    if (!stats) {
      stats = { totalMs: 0, maxMs: 0, count: 0 }
      counterStats.set(name, stats)
    }
    stats.totalMs += value
    stats.maxMs = Math.max(stats.maxMs, value)
    stats.count += 1
  }

  function onFrame(fixedDeltaMs: number, frameTimeMs: number) {
    if (!isEnabled()) return

    frameCount += 1
    rawDeltaTotal += fixedDeltaMs
    rawDeltaMax = Math.max(rawDeltaMax, fixedDeltaMs)

    if (fixedDeltaMs > frameTimeMs * FRAME_DROP_THRESHOLD_MULTIPLIER) {
      frameDropCount += 1
    }
  }

  function record(section: LoopProfileSection, durationMs: number) {
    if (!isEnabled()) return

    const stats = statsBySection.get(section)
    if (!stats) return

    stats.totalMs += durationMs
    stats.maxMs = Math.max(stats.maxMs, durationMs)
    stats.count += 1
  }

  function flush(now: number) {
    if (!isEnabled()) return

    const elapsed = now - windowStart
    if (elapsed < PROFILE_WINDOW_MS || frameCount === 0) return

    const rows = LOOP_PROFILE_SECTIONS.map((section) => {
      const stats = statsBySection.get(section)
      const averageMs =
        stats && stats.count > 0 ? stats.totalMs / stats.count : 0

      return {
        section,
        avg_ms: Number(averageMs.toFixed(3)),
        max_ms: Number((stats?.maxMs ?? 0).toFixed(3)),
        samples: stats?.count ?? 0,
      }
    })

    const avgRawDelta = rawDeltaTotal / frameCount
    const fps = frameCount > 0 ? 1000 / avgRawDelta : 0
    console.groupCollapsed(
      `[LoopProfile] fps=${fps.toFixed(1)} frames=${frameCount} dropped=${frameDropCount} avgDelta=${avgRawDelta.toFixed(2)}ms maxDelta=${rawDeltaMax.toFixed(2)}ms`
    )
    console.table(rows)

    if (counterStats.size > 0) {
      const counterRows = Array.from(counterStats.entries()).map(
        ([name, stats]) => ({
          counter: name,
          avg_per_frame:
            stats.count > 0
              ? Number((stats.totalMs / stats.count).toFixed(1))
              : 0,
          max: Number(stats.maxMs.toFixed(1)),
          samples: stats.count,
        })
      )
      console.table(counterRows)
    }
    console.groupEnd()

    resetWindow(now)
  }

  return {
    resetWindow,
    onFrame,
    record,
    recordCount,
    flush,
  }
}
