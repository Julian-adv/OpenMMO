import { describe, expect, it } from 'vitest'
import {
  createSecondaryMotionScheduler,
  REMOTE_PHYSICS_STEP,
} from './secondary-motion-scheduler'

describe('secondary motion scheduling', () => {
  it.each([30, 60, 144])(
    'keeps remote solves at 30 Hz on a %i Hz render loop',
    (fps) => {
      const scheduler = createSecondaryMotionScheduler()
      scheduler.next(1 / fps, true, false)
      let updates = 0
      let elapsed = 0
      for (let i = 0; i < fps * 2; i++) {
        const step = scheduler.next(1 / fps, true, false)
        if (!step) continue
        updates++
        elapsed += step.deltaTime
        expect(step.reset).toBe(false)
      }
      expect(updates).toBe(60)
      expect(elapsed).toBeCloseTo(2, 8)
    }
  )

  it('updates the local player every frame without accumulating skipped time', () => {
    const scheduler = createSecondaryMotionScheduler()
    for (let i = 0; i < 120; i++)
      expect(scheduler.next(1 / 60, true, true)?.deltaTime).toBe(1 / 60)
  })

  it('spreads remote solves across alternating frames', () => {
    const a = createSecondaryMotionScheduler(2)
    const b = createSecondaryMotionScheduler(3)
    a.next(1 / 60, true, false)
    b.next(1 / 60, true, false)
    for (let frame = 1; frame < 10; frame++) {
      const even = a.next(1 / 60, true, false)
      const odd = b.next(1 / 60, true, false)
      if (frame >= 2) expect(!!even).toBe(!odd)
    }
  })

  it('pauses hidden characters and resets them when visible again', () => {
    const scheduler = createSecondaryMotionScheduler()
    scheduler.next(1 / 60, true, false)
    for (let i = 0; i < 120; i++)
      expect(scheduler.next(1 / 60, false, false)).toBeNull()
    const resumed = scheduler.next(1 / 60, true, false)
    expect(resumed).toEqual({
      deltaTime: 1 / 60,
      reset: true,
      step: REMOTE_PHYSICS_STEP,
    })
  })

  it('resets after a long frame instead of running an unbounded catch-up', () => {
    const scheduler = createSecondaryMotionScheduler()
    scheduler.next(1 / 60, true, false)
    expect(scheduler.next(2, true, false)).toEqual({
      deltaTime: 2,
      reset: true,
      step: REMOTE_PHYSICS_STEP,
    })
  })
})
