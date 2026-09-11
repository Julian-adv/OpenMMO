import type { TimestampSample } from './metrics'

const palette = ['#167b6c', '#5276d1', '#c66a2b', '#9a59bb', '#c34f74', '#708c2f', '#3194a8', '#aa8242', '#6861a8', '#885b51']

export function createCharacterColors() {
  let previous: Record<string, string> = Object.create(null)
  return (names: string[]) => {
    const used = new Set(names.map((name) => previous[name]).filter(Boolean))
    const colors: Record<string, string> = Object.create(null)
    for (const name of names) {
      colors[name] = previous[name] ?? palette.find((color) => !used.has(color))!
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
