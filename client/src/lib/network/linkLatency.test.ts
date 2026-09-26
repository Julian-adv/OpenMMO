import { describe, expect, it } from 'vitest'
import { LinkLatency } from './linkLatency'

/** Drives a tracker through a link with a fixed one-way cost per direction. */
function harness(options: { rttMs: number }) {
  const link = new LinkLatency()
  let rttMs = options.rttMs
  let seq = 0
  let now = 10_000
  const sent: { seq: number; at: number }[] = []

  const pump = () => {
    link.due(now, (s, clientTimeMs) => {
      seq = s
      sent.push({ seq: s, at: now })
      expect(clientTimeMs).toBe(Math.round(now))
    })
  }

  const answer = (atSeq = seq) => {
    const sentAt = sent.find((p) => p.seq === atSeq)?.at
    if (sentAt === undefined) return false
    return link.accept(atSeq, Math.round(sentAt), sentAt + rttMs)
  }

  return {
    link,
    pump,
    answer,
    setRtt: (ms: number) => {
      rttMs = ms
    },
    advance: (ms: number) => {
      now += ms
    },
    now: () => now,
  }
}

describe('LinkLatency', () => {
  it('reports nothing until a probe is answered', () => {
    const { link, pump } = harness({ rttMs: 200 })
    pump()
    expect(link.rttMs).toBe(0)
    expect(link.oneWayMs).toBe(0)
    expect(link.sample).toBeNull()
  })

  it.each([20, 150, 250, 400])('halves a %ims round trip', (rttMs) => {
    const h = harness({ rttMs })
    h.pump()
    expect(h.answer()).toBe(true)
    expect(h.link.rttMs).toBeCloseTo(rttMs, 5)
    expect(h.link.oneWayMs).toBeCloseTo(rttMs / 2, 5)
  })

  it('paces probes so one player is not a message flood', () => {
    const h = harness({ rttMs: 100 })
    h.pump()
    h.answer()
    for (let i = 0; i < 50; i++) {
      h.advance(10)
      h.pump()
    }
    expect(h.link.rttMs).toBeCloseTo(100, 5)
  })

  it('rejects an answer whose echoed time does not match the probe', () => {
    const h = harness({ rttMs: 100 })
    h.pump()
    expect(h.link.accept(999, 10_000, 10_100)).toBe(false)
    expect(h.link.rttMs).toBe(0)
  })

  it('rejects a reordered answer for a probe it already retired', () => {
    const h = harness({ rttMs: 100 })
    h.pump()
    h.answer()
    const stale = h.link.rttMs
    expect(h.link.accept(1, 10_000, 10_400)).toBe(false)
    expect(h.link.rttMs).toBe(stale)
  })

  it('folds a slow sample in gradually rather than jumping the estimate', () => {
    const h = harness({ rttMs: 100 })
    h.pump()
    h.answer()
    expect(h.link.rttMs).toBeCloseTo(100, 5)
    // A route change to a much worse path: the estimate must move, but only
    // part of the way, so one bad sample cannot inflate playback lag.
    h.setRtt(400)
    h.advance(2100)
    h.pump()
    h.answer()
    expect(h.link.rttMs).toBeGreaterThan(100)
    expect(h.link.rttMs).toBeLessThan(400)
    expect(h.link.rttMs).toBeCloseTo(175, 5)
  })

  it('recovers from a spike instead of staying inflated', () => {
    const link = new LinkLatency()
    let now = 0
    const sentAt: number[] = []
    const pump = () =>
      link.due(now, () => {
        sentAt.push(now)
      })
    const answer = (rtt: number) => {
      const at = sentAt.at(-1)!
      link.accept(sentAt.length, at, at + rtt)
    }
    for (const rtt of [100, 100, 100, 900, 100, 100, 100, 100, 100, 100]) {
      pump()
      answer(rtt)
      now += 2100
    }
    // Smoothing with alpha 0.25 leaves the estimate well below the spike.
    expect(link.rttMs).toBeLessThan(300)
    expect(link.jitterMs).toBeGreaterThan(0)
  })

  it('forgets everything on reset, as a new socket requires', () => {
    const h = harness({ rttMs: 250 })
    h.pump()
    h.answer()
    expect(h.link.rttMs).toBeGreaterThan(0)
    h.link.reset()
    expect(h.link.rttMs).toBe(0)
    expect(h.link.sample).toBeNull()
  })

  it('lets a probe go out again immediately after a reset', () => {
    const h = harness({ rttMs: 250 })
    h.pump()
    h.link.reset()
    h.pump()
    expect(h.answer()).toBe(true)
  })

  it('reports spread across the window as jitter', () => {
    const link = new LinkLatency()
    const sentAt: number[] = []
    let now = 0
    const pump = () => link.due(now, () => void sentAt.push(now))
    const answer = (rtt: number) => {
      const at = sentAt.at(-1)!
      link.accept(sentAt.length, at, at + rtt)
    }
    for (const rtt of [100, 100, 100]) {
      pump()
      answer(rtt)
      now += 2100
    }
    expect(link.jitterMs).toBe(0)
  })
})
