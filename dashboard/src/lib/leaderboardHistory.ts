import type { TimestampSample } from './metrics'

const palette = ['#167b6c', '#5276d1', '#c66a2b', '#9a59bb', '#c34f74', '#708c2f', '#3194a8', '#aa8242', '#6861a8', '#885b51']

export function createCharacterColors() {
  let previous: Record<string, string> = Object.create(null)
  let nextColor = 0
  return (names: string[]) => {
    const used = new Set(names.map((name) => previous[name]).filter(Boolean))
    const colors: Record<string, string> = Object.create(null)
    for (const name of names) {
      colors[name] = previous[name] ?? palette.find((color) => !used.has(color)) ?? `hsl(${(nextColor++ * 137.508) % 360} 55% 40%)`
      used.add(colors[name])
    }
    previous = colors
    return colors
  }
}

export function sampleAt<T extends TimestampSample>(samples: T[], timestamp: number): T | null {
  let low = 0
  let high = samples.length
  while (low < high) {
    const middle = Math.floor((low + high) / 2)
    if (samples[middle].timestamp <= timestamp) low = middle + 1
    else high = middle
  }
  return samples[low - 1] ?? null
}

export function stepPath<T extends TimestampSample>(samples: T[], until: number, x: (time: number) => number, y: (value: number) => number, value: (sample: T) => number) {
  return samples.map((sample, index) => index === 0
    ? `M${x(sample.timestamp)},${y(value(sample))}`
    : `H${x(sample.timestamp)}V${y(value(sample))}`).join(' ') + (samples.length ? ` H${x(until)}` : '')
}

export interface StepChange {
  name: string
  timestamp: number
  before: number
  after: number
}

export function stepChangesAt<T extends TimestampSample>(series: { name: string, samples: T[] }[], point: { x: number, y: number }, x: (time: number) => number, y: (value: number) => number, value: (sample: T) => number, tolerance = 6): StepChange[] {
  let nearest = tolerance ** 2
  let changes: StepChange[] = []
  for (const entry of series) {
    for (let index = 1; index < entry.samples.length; index++) {
      const sample = entry.samples[index]
      const before = value(entry.samples[index - 1])
      const after = value(sample)
      if (before === after) continue
      const dxSquared = (x(sample.timestamp) - point.x) ** 2
      if (dxSquared > nearest) continue
      const fromY = y(before)
      const toY = y(after)
      const dy = Math.max(Math.min(fromY, toY) - point.y, 0, point.y - Math.max(fromY, toY))
      const distance = dxSquared + dy ** 2
      if (distance > nearest) continue
      if (distance < nearest) {
        nearest = distance
        changes = []
      }
      if (changes.length === 0 || changes[0].timestamp === sample.timestamp) {
        changes.push({ name: entry.name, timestamp: sample.timestamp, before, after })
      }
    }
  }
  return changes
}
