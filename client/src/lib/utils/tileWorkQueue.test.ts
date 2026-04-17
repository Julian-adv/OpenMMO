import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { enqueueTileWork, drainTileWork } from './tileWorkQueue'

beforeEach(() => {
  // Clear residual state from prior tests by draining with a huge budget.
  drainTileWork(Number.POSITIVE_INFINITY)
})

describe('drainTileWork — empty queue', () => {
  it('is a no-op when nothing is queued', () => {
    // Should not throw and should not call performance.now (fast-path return).
    const spy = vi.spyOn(performance, 'now')
    drainTileWork(4)
    expect(spy).not.toHaveBeenCalled()
    spy.mockRestore()
  })
})

describe('drainTileWork — execution order', () => {
  it('runs items FIFO', () => {
    const log: number[] = []
    enqueueTileWork(() => log.push(1))
    enqueueTileWork(() => log.push(2))
    enqueueTileWork(() => log.push(3))
    drainTileWork(Number.POSITIVE_INFINITY)
    expect(log).toEqual([1, 2, 3])
  })

  it('runs each item exactly once across multiple drains', () => {
    const log: number[] = []
    enqueueTileWork(() => log.push(1))
    enqueueTileWork(() => log.push(2))
    drainTileWork(Number.POSITIVE_INFINITY)
    drainTileWork(Number.POSITIVE_INFINITY)
    expect(log).toEqual([1, 2])
  })
})

describe('drainTileWork — time budget', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('stops processing once the deadline passes', () => {
    // Simulate performance.now returning: start=0, then 1, 2, 3, ... per check.
    // With budgetMs=2, deadline=2. While-loop reads now < 2 → runs 2 items,
    // then now=2 is not < 2, stops.
    let nowValue = 0
    vi.spyOn(performance, 'now').mockImplementation(() => nowValue++)

    const log: number[] = []
    for (let i = 0; i < 10; i++) {
      enqueueTileWork(() => log.push(i))
    }
    drainTileWork(2)
    expect(log.length).toBeLessThan(10)
    expect(log.length).toBeGreaterThan(0)

    // Remaining items drain on a subsequent call with a big budget.
    nowValue = 0
    vi.spyOn(performance, 'now').mockImplementation(() => 0)
    drainTileWork(Number.POSITIVE_INFINITY)
    expect(log.length).toBe(10)
  })

  it('processes at least one item even with a tiny budget', () => {
    // Real timing: with budget 4ms, at least the first item should run.
    const log: number[] = []
    enqueueTileWork(() => log.push(1))
    drainTileWork(4)
    expect(log).toEqual([1])
  })
})

describe('drainTileWork — compaction', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('fully drains when budget allows, enabling fresh enqueues to run', () => {
    const log: string[] = []
    enqueueTileWork(() => log.push('a'))
    drainTileWork(Number.POSITIVE_INFINITY)
    enqueueTileWork(() => log.push('b'))
    drainTileWork(Number.POSITIVE_INFINITY)
    expect(log).toEqual(['a', 'b'])
  })

  it('handles partial drain followed by additional enqueues', () => {
    // Run only the first 2 of 5 items, then enqueue 2 more, then drain all.
    let nowValue = 0
    const spy = vi
      .spyOn(performance, 'now')
      .mockImplementation(() => nowValue++)

    const log: number[] = []
    for (let i = 0; i < 5; i++) enqueueTileWork(() => log.push(i))
    // Mock sequence: deadline-read=0, then per-iter reads 1,2,3,...
    // deadline=3 → iterations with now=1 and now=2 pass; now=3 stops.
    drainTileWork(3)
    expect(log).toEqual([0, 1])

    spy.mockRestore()
    enqueueTileWork(() => log.push(5))
    enqueueTileWork(() => log.push(6))
    drainTileWork(Number.POSITIVE_INFINITY)
    expect(log).toEqual([0, 1, 2, 3, 4, 5, 6])
  })

  it('compacts when head exceeds 128 with remaining items', () => {
    // Queue 200 items, then in one drain process only 130 (head=130 > 128).
    // After that drain, head should be reset so the remaining 70 still run.
    let nowValue = 0
    const spy = vi.spyOn(performance, 'now').mockImplementation(() => {
      // Deadline call returns 0 → deadline=budget. Each check increments.
      return nowValue++
    })

    const log: number[] = []
    for (let i = 0; i < 200; i++) enqueueTileWork(() => log.push(i))

    // First drain: budget 130 → runs ~130 items before deadline.
    drainTileWork(130)
    expect(log.length).toBeGreaterThan(128)
    const firstCount = log.length
    expect(firstCount).toBeLessThan(200)

    spy.mockRestore()
    drainTileWork(Number.POSITIVE_INFINITY)
    expect(log.length).toBe(200)
    // Validate ordering is preserved across compaction.
    expect(log).toEqual([...Array(200).keys()])
  })
})

describe('enqueueTileWork — reentrancy', () => {
  it('allows a work item to enqueue more items (processed on next drain)', () => {
    const log: string[] = []
    enqueueTileWork(() => {
      log.push('outer')
      enqueueTileWork(() => log.push('inner'))
    })
    drainTileWork(Number.POSITIVE_INFINITY)
    // Both should run: the inner item gets drained within the same loop
    // because `queue.length` is re-evaluated each iteration.
    expect(log).toEqual(['outer', 'inner'])
  })
})
