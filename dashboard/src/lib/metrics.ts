export interface Sample {
  timestamp: number
  accounts: number
}

export interface ConcurrentHistory {
  from: number
  until: number
  sample_interval_seconds: number
  current: Sample
  samples: Sample[]
}

export type Hours = 1 | 6 | 24

function isSample(value: unknown): value is Sample {
  if (!value || typeof value !== 'object') return false
  const sample = value as Sample
  return Number.isSafeInteger(sample.timestamp) && sample.timestamp >= 0 &&
    Number.isSafeInteger(sample.accounts) && sample.accounts >= 0
}

export function parseHistory(value: unknown, hours: Hours): ConcurrentHistory {
  if (!value || typeof value !== 'object') throw new Error('Invalid metrics response')
  const data = value as ConcurrentHistory
  if (!Number.isSafeInteger(data.from) || !Number.isSafeInteger(data.until) ||
    data.until - data.from !== hours * 3600 || data.sample_interval_seconds !== 60 ||
    !isSample(data.current) || data.current.timestamp !== data.until ||
    !Array.isArray(data.samples) || data.samples.length > hours * 60 + 1 ||
    !data.samples.every((sample, index) => isSample(sample) &&
      sample.timestamp >= data.from && sample.timestamp <= data.until &&
      (index === 0 || sample.timestamp > data.samples[index - 1].timestamp))) {
    throw new Error('Invalid metrics response')
  }
  return data
}

export function summarize(samples: Sample[]) {
  if (samples.length === 0) return { peak: null, average: null, peakAt: null }
  let peak = samples[0]
  let total = 0
  for (const sample of samples) {
    if (sample.accounts > peak.accounts) peak = sample
    total += sample.accounts
  }
  return { peak: peak.accounts, average: total / samples.length, peakAt: peak.timestamp }
}

export function splitSegments(samples: Sample[], interval: number): Sample[][] {
  const segments: Sample[][] = []
  for (const sample of samples) {
    const previous = segments.at(-1)?.at(-1)
    if (!previous || sample.timestamp - previous.timestamp > interval * 1.5) {
      segments.push([sample])
    } else {
      segments[segments.length - 1].push(sample)
    }
  }
  return segments
}

export function nearestSample(samples: Sample[], timestamp: number): number | null {
  if (!samples.length) return null
  let low = 0
  let high = samples.length - 1
  while (low < high) {
    const middle = Math.floor((low + high) / 2)
    if (samples[middle].timestamp < timestamp) low = middle + 1
    else high = middle
  }
  return low > 0 && timestamp - samples[low - 1].timestamp <= samples[low].timestamp - timestamp
    ? low - 1 : low
}

const timeFormatter = new Intl.DateTimeFormat('ko-KR', {
  timeZone: 'Asia/Seoul', hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
})
const dateFormatter = new Intl.DateTimeFormat('ko-KR', {
  timeZone: 'Asia/Seoul', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
})

export const formatTime = (timestamp: number) => timeFormatter.format(timestamp * 1000)
export const formatDateTime = (timestamp: number) => dateFormatter.format(timestamp * 1000)
