const SPIKE = 12
const SPIKE_LENGTHS = [1, 0.55, 0.85, 0.6, 1, 0.7, 0.9, 0.5]
const TAIL = 20
const SPIKE_SPACING = 22
const POWER = 3
const FIT = 2 ** (1 / POWER) * 0.94
const SAMPLES = 240

/** How far the tail of a burst around a box this tall drops below it. */
export function speechBurstDrop(boxH: number, scale: number): number {
  return (boxH / 2) * (FIT - 1) + TAIL * scale
}

/** Comic shout outline: spikes evenly spaced along a superellipse hugging
 *  the box, with one long spike pointing straight down at the speaker. */
export function speechBurstPath(
  boxW: number,
  boxH: number,
  scale: number
): string {
  const rx = (boxW / 2) * FIT
  const ry = (boxH / 2) * FIT
  const pow = (v: number, e: number) => Math.sign(v) * Math.abs(v) ** e
  const outline = Array.from({ length: SAMPLES + 1 }, (_, i) => {
    const t = Math.PI / 2 + (i / SAMPLES) * Math.PI * 2
    const c = Math.cos(t)
    const s = Math.sin(t)
    const nx = pow(c, 2 - 2 / POWER) / rx
    const ny = pow(s, 2 - 2 / POWER) / ry
    const n = Math.hypot(nx, ny)
    return {
      x: boxW / 2 + rx * pow(c, 2 / POWER),
      y: boxH / 2 + ry * pow(s, 2 / POWER),
      nx: nx / n,
      ny: ny / n,
    }
  })
  const lengths = [0]
  for (let i = 1; i <= SAMPLES; i++) {
    lengths.push(
      lengths[i - 1] +
        Math.hypot(
          outline[i].x - outline[i - 1].x,
          outline[i].y - outline[i - 1].y
        )
    )
  }
  const total = lengths[SAMPLES]
  const spikes = Math.max(
    10,
    Math.min(40, Math.round(total / (SPIKE_SPACING * scale)))
  )
  const points: string[] = []
  let cursor = 0
  for (let k = 0; k < spikes * 2; k++) {
    const distance = (k / (spikes * 2)) * total
    while (cursor < SAMPLES - 1 && lengths[cursor + 1] < distance) cursor++
    const p = outline[cursor]
    const spike = k / 2
    const reach =
      k % 2
        ? 0
        : (spike ? SPIKE * SPIKE_LENGTHS[spike % SPIKE_LENGTHS.length] : TAIL) *
          scale
    points.push(
      `${(p.x + p.nx * reach).toFixed(1)} ${(p.y + p.ny * reach).toFixed(1)}`
    )
  }
  return `M${points.join('L')}Z`
}
