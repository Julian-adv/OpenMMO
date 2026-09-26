import { afterEach, describe, expect, it, vi } from 'vitest'

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('frame preparation scheduling', () => {
  it('resumes one waiting task after paint instead of all tasks in one frame', async () => {
    vi.resetModules()
    vi.useFakeTimers()
    const frames: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      frames.push(callback)
      return frames.length
    })
    const { yieldTask } = await import('./frameYield')
    const resumed: number[] = []
    const jobs = [1, 2, 3].map((id) => yieldTask().then(() => resumed.push(id)))
    expect(frames).toHaveLength(1)
    for (let id = 1; id <= 3; id++) {
      frames.shift()!(id * 16)
      expect(resumed).toHaveLength(id - 1)
      await vi.runOnlyPendingTimersAsync()
      expect(resumed).toEqual([1, 2, 3].slice(0, id))
      expect(frames).toHaveLength(id < 3 ? 1 : 0)
    }
    await Promise.all(jobs)
  })

  it('keeps working without a browser animation frame API', async () => {
    vi.resetModules()
    vi.useFakeTimers()
    vi.stubGlobal('requestAnimationFrame', undefined)
    const { yieldTask } = await import('./frameYield')
    const resumed = vi.fn()
    const work = yieldTask().then(resumed)
    expect(resumed).not.toHaveBeenCalled()
    await vi.runAllTimersAsync()
    await work
    expect(resumed).toHaveBeenCalledOnce()
  })
})
