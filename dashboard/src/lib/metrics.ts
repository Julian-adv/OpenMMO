export const connectionKinds = [
  { key: 'web_accounts', label: '웹 접속', color: '#168878' },
  { key: 'agent_accounts', label: '외부 에이전트', color: '#7870cf' },
  { key: 'other_accounts', label: '기타·미분류', color: '#9aa6af' },
] as const

export type ConnectionCounts = Record<typeof connectionKinds[number]['key'], number>

export interface Sample extends ConnectionCounts {
  timestamp: number
  accounts: number
}

export interface HistorySample extends Sample {
  peak_accounts: number
  peak_timestamp: number
  sample_count: number
}

export interface ConcurrentHistory {
  from: number
  until: number
  sample_interval_seconds: number
  current: Sample
  samples: HistorySample[]
}

export const periods = [
  { hours: 1, label: '1시간', interval: 60, intervalLabel: '1분 간격' },
  { hours: 6, label: '6시간', interval: 60, intervalLabel: '1분 간격' },
  { hours: 24, label: '24시간', interval: 60, intervalLabel: '1분 간격' },
  { hours: 168, label: '1주일', interval: 600, intervalLabel: '10분 평균' },
  { hours: 720, label: '1개월', interval: 3600, intervalLabel: '1시간 평균' },
  { hours: 4320, label: '6개월', interval: 21600, intervalLabel: '6시간 평균' },
  { hours: 8760, label: '1년', interval: 86400, intervalLabel: '1일 평균' },
] as const

export type Hours = typeof periods[number]['hours']
export const formatPeriod = (hours: number) => periods.find((period) => period.hours === hours)?.label ?? `${hours}시간`

export function connectionParts(sample: Sample) {
  return connectionKinds.map((kind) => ({
    ...kind,
    accounts: sample[kind.key],
    percent: sample.accounts > 0 ? sample[kind.key] / sample.accounts * 100 : 0,
  }))
}

export function visibleConnectionParts(sample: Sample) {
  return connectionParts(sample).filter((part) => part.key !== 'other_accounts' || part.accounts > 0)
}

export function formatBreakdown(sample: Sample) {
  return visibleConnectionParts(sample)
    .map((part) => `${part.label} ${formatCount(part.accounts)}계정 (${formatCount(part.percent)}%)`).join(', ')
}

function hasValidCounts(sample: Sample, integer: boolean) {
  return connectionKinds.every(({ key }) => Number.isFinite(sample[key]) && sample[key] >= 0 &&
    (!integer || Number.isSafeInteger(sample[key]))) &&
    Math.abs(sample.web_accounts + sample.agent_accounts + sample.other_accounts - sample.accounts) <= Math.max(1, sample.accounts) * 1e-9
}

function isSample(value: unknown): value is Sample {
  if (!value || typeof value !== 'object') return false
  const sample = value as Sample
  return Number.isSafeInteger(sample.timestamp) && sample.timestamp >= 0 &&
    Number.isSafeInteger(sample.accounts) && sample.accounts >= 0 && hasValidCounts(sample, true)
}

export function parseHistory(value: unknown, hours: Hours): ConcurrentHistory {
  if (!value || typeof value !== 'object') throw new Error('Invalid metrics response')
  const data = value as ConcurrentHistory
  const interval = periods.find((period) => period.hours === hours)!.interval
  if (!Number.isSafeInteger(data.from) || !Number.isSafeInteger(data.until) ||
    data.until - data.from !== hours * 3600 || data.sample_interval_seconds !== interval ||
    !isSample(data.current) || data.current.timestamp !== data.until ||
    !Array.isArray(data.samples) || data.samples.length > Math.ceil(hours * 3600 / interval) + 1 ||
    !data.samples.every((sample, index) => sample && Number.isSafeInteger(sample.timestamp) &&
      Number.isFinite(sample.accounts) && sample.accounts >= 0 && hasValidCounts(sample, false) &&
      Number.isSafeInteger(sample.peak_accounts) && sample.peak_accounts >= sample.accounts &&
      Number.isSafeInteger(sample.peak_timestamp) && sample.peak_timestamp >= sample.timestamp &&
      sample.peak_timestamp < sample.timestamp + interval && sample.peak_timestamp <= data.until &&
      Number.isSafeInteger(sample.sample_count) && sample.sample_count > 0 && sample.sample_count <= interval / 60 &&
      sample.timestamp >= data.from && sample.timestamp <= data.until &&
      (index === 0 || sample.timestamp > data.samples[index - 1].timestamp))) {
    throw new Error('Invalid metrics response')
  }
  return data
}

export function summarize(samples: HistorySample[]) {
  if (samples.length === 0) return { peak: null, average: null, peakAt: null, sampleCount: 0 }
  let peak = samples[0]
  let total = 0
  let sampleCount = 0
  for (const sample of samples) {
    if (sample.peak_accounts > peak.peak_accounts) peak = sample
    total += sample.accounts * sample.sample_count
    sampleCount += sample.sample_count
  }
  return { peak: peak.peak_accounts, average: total / sampleCount, peakAt: peak.peak_timestamp, sampleCount }
}

export function splitSegments(samples: Sample[], interval: number): Sample[][] {
  const segments: Sample[][] = []
  for (const sample of samples) {
    const previous = segments.at(-1)?.at(-1)
    if (!previous || Math.floor(sample.timestamp / interval) - Math.floor(previous.timestamp / interval) > 1) {
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
  timeZone: 'Asia/Seoul', year: 'numeric', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
})
const dayFormatter = new Intl.DateTimeFormat('ko-KR', {
  timeZone: 'Asia/Seoul', month: 'numeric', day: 'numeric',
})
const monthFormatter = new Intl.DateTimeFormat('ko-KR', {
  timeZone: 'Asia/Seoul', year: '2-digit', month: 'numeric',
})

export const formatTime = (timestamp: number) => timeFormatter.format(timestamp * 1000)
export const formatCount = (value: number) => value.toLocaleString('ko-KR', { maximumFractionDigits: 1 })
export const formatDateTime = (timestamp: number) => dateFormatter.format(timestamp * 1000)
export const formatAxisTime = (timestamp: number, hours: number) =>
  (hours <= 24 ? timeFormatter : hours <= 4320 ? dayFormatter : monthFormatter).format(timestamp * 1000)
