/// How often a probe goes out. Fast enough to notice a route change (a roaming
/// client, a VPN toggle), rare enough that a 5,000-player server sees one
/// message per player per interval rather than a flood.
const PROBE_INTERVAL_MS = 2000

/// A probe older than this never produced an answer; count it as loss so a
/// link that only answers half the time does not look idle-fast.
const PROBE_TIMEOUT_MS = 6000

/// Samples kept for the floor. The minimum round trip over a window is the
/// least-delayed sample the link produced, which is the honest basis for
/// compensation: a spike should widen the error band, not move the estimate.
const WINDOW = 8

/// Time constant for the smoothed estimate. Fast enough to follow a route
/// change within a couple of probes, slow enough that one spike does not
/// drag the whole estimate up.
const SMOOTHING = 0.25

export type LinkSample = {
  rttMs: number
  /** Spread between the fastest and slowest sample in the window. */
  jitterMs: number
}

/**
 * Round-trip estimator for the websocket link.
 *
 * The client cannot otherwise know how much of a delay is spent in each
 * direction, and that is what playback needs: `oneWayMs` is the amount the
 * anchor is dated back by. Anchoring on arrival instead trails the
 * authoritative position by a whole one-way trip for as long as the walk
 * lasts, which is the difference between smooth and rubber-banding on a link
 * that crosses an ocean.
 *
 * Deliberately not tracking a clock offset. `server_time_ms` is an epoch
 * stamp while the local clock is monotonic from an arbitrary origin, so the
 * difference between them is a large constant rather than a usable offset;
 * measuring against a live server confirmed it. Until something needs a real
 * clock sync, the round trip is the honest and sufficient number.
 */
export class LinkLatency {
  private rtts: number[] = []
  private smoothedRtt = 0
  private seeded = false
  private nextProbeAt = 0
  private seq = 0
  private readonly pending = new Map<number, number>()

  /** Round-trip estimate, or 0 until the first answer lands. */
  get rttMs(): number {
    return this.seeded ? this.smoothedRtt : 0
  }

  /** Half the round trip: the best available estimate of one direction. */
  get oneWayMs(): number {
    return this.rttMs / 2
  }

  /** Spread of the recent window. A wide spread means corrections will show. */
  get jitterMs(): number {
    if (this.rtts.length < 2) return 0
    return Math.max(...this.rtts) - Math.min(...this.rtts)
  }

  get sample(): LinkSample | null {
    return this.seeded
      ? { rttMs: this.smoothedRtt, jitterMs: this.jitterMs }
      : null
  }

  /**
   * A probe is due. `now` is the local monotonic clock; the caller supplies
   * the send so a probe is only claimed when it really goes on the wire.
   */
  due(now: number, send: (seq: number, clientTimeMs: number) => void): boolean {
    this.expire(now)
    if (now < this.nextProbeAt) return false
    this.nextProbeAt = now + PROBE_INTERVAL_MS
    const seq = ++this.seq
    this.pending.set(seq, now)
    send(seq, Math.round(now))
    return true
  }

  /**
   * Fold an answer in. `clientTimeMs` is echoed back so a reordered or stale
   * answer cannot be mistaken for the probe it claims to be; an unknown seq
   * is dropped rather than guessed at.
   */
  accept(seq: number, clientTimeMs: number, now: number): boolean {
    const sentAt = this.pending.get(seq)
    if (sentAt === undefined || Math.round(sentAt) !== clientTimeMs)
      return false
    this.pending.delete(seq)
    const rtt = now - sentAt
    if (rtt < 0) return false
    if (this.seeded) this.smoothedRtt += (rtt - this.smoothedRtt) * SMOOTHING
    else {
      this.smoothedRtt = rtt
      this.seeded = true
    }
    this.rtts.push(rtt)
    if (this.rtts.length > WINDOW) this.rtts.shift()
    return true
  }

  reset() {
    this.rtts.length = 0
    this.pending.clear()
    this.smoothedRtt = 0
    this.seeded = false
    this.nextProbeAt = 0
  }

  private expire(now: number) {
    for (const [seq, sentAt] of this.pending) {
      if (now - sentAt >= PROBE_TIMEOUT_MS) this.pending.delete(seq)
    }
  }
}
