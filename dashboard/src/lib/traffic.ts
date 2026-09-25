export const trafficPeriods = [
  { hours: 1, label: '1시간' },
  { hours: 24, label: '24시간' },
  { hours: 168, label: '7일' },
  { hours: 720, label: '30일' },
] as const
export type TrafficHours = typeof trafficPeriods[number]['hours']

export interface NetworkPoint {
  timestamp: number
  seconds: number
  rx_bytes: number
  tx_bytes: number
}

export interface NetworkHistory {
  from: number
  until: number
  collection_started_at: number | null
  collection_interval_seconds: number
  sample_interval_seconds: number
  available: boolean
  latest: (NetworkPoint & { interface: string, accounts: number }) | null
  samples: NetworkPoint[]
  rx_bytes: number
  tx_bytes: number
}

export const categoryLabels = {
  bgm: 'BGM', sound: '효과음·오디오', texture: '텍스처·이미지', model: 'GLB·모델',
  code: 'JS·CSS·WASM', font: '폰트', other: '기타 정적 파일',
}

export interface AssetEntry {
  category: keyof typeof categoryLabels
  path: string
  bytes: number
  requests: number
  revalidations: number
}

export interface AssetTraffic {
  from: number
  until: number
  configured: boolean
  available: boolean
  status: {
    started_at: number
    updated_at: number
    skipped_lines: number
    gaps: number
    pending_bytes: number
  } | null
  total_bytes: number
  categories: AssetEntry[]
  files: AssetEntry[]
}

const count = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0
const point = (value: NetworkPoint) => value && count(value.timestamp) && Number.isFinite(value.seconds) && value.seconds > 0 && count(value.rx_bytes) && count(value.tx_bytes)

export function parseNetworkHistory(value: unknown, hours: TrafficHours): NetworkHistory {
  if (!value || typeof value !== 'object') throw new Error('Invalid network history')
  const data = value as NetworkHistory
  if (!count(data.from) || !count(data.until) || data.until - data.from !== hours * 3600 ||
    !count(data.collection_interval_seconds) || data.collection_interval_seconds < 60 ||
    !count(data.sample_interval_seconds) || data.sample_interval_seconds < data.collection_interval_seconds ||
    typeof data.available !== 'boolean' || !count(data.rx_bytes) || !count(data.tx_bytes) ||
    !(data.collection_started_at === null || count(data.collection_started_at) && data.collection_started_at <= data.until) ||
    !(data.latest === null || point(data.latest) && data.latest.timestamp <= data.until && typeof data.latest.interface === 'string' && count(data.latest.accounts)) ||
    !Array.isArray(data.samples) || data.samples.length > Math.ceil(hours * 3600 / data.sample_interval_seconds) + 1 ||
    !data.samples.every((sample, index) => point(sample) && sample.timestamp > data.from && sample.timestamp <= data.until &&
      (index === 0 || sample.timestamp > data.samples[index - 1].timestamp)) ||
    data.rx_bytes !== data.samples.reduce((sum, sample) => sum + sample.rx_bytes, 0) ||
    data.tx_bytes !== data.samples.reduce((sum, sample) => sum + sample.tx_bytes, 0)) {
    throw new Error('Invalid network history')
  }
  return data
}

export function parseAssetTraffic(value: unknown, hours: TrafficHours): AssetTraffic {
  if (!value || typeof value !== 'object') throw new Error('Invalid asset traffic')
  const data = value as AssetTraffic
  const entries = (rows: AssetEntry[], files: boolean) => Array.isArray(rows) && rows.length <= (files ? 20 : Object.keys(categoryLabels).length) &&
    rows.every((row, index) => row && Object.hasOwn(categoryLabels, row.category) && typeof row.path === 'string' &&
      (files ? row.path.startsWith('/') : row.path === '') && count(row.bytes) && count(row.requests) && count(row.revalidations) &&
      row.revalidations <= row.requests && (index === 0 || row.bytes <= rows[index - 1].bytes)) &&
    new Set(rows.map((row) => files ? row.path : row.category)).size === rows.length
  if (!count(data.from) || !count(data.until) || data.until - data.from !== hours * 3600 ||
    typeof data.configured !== 'boolean' || typeof data.available !== 'boolean' || !count(data.total_bytes) ||
    !(data.status === null || data.status && count(data.status.started_at) && count(data.status.updated_at) &&
      data.status.updated_at >= data.status.started_at && count(data.status.skipped_lines) && count(data.status.gaps) && count(data.status.pending_bytes)) ||
    !entries(data.categories, false) || !entries(data.files, true) ||
    data.total_bytes !== data.categories.reduce((sum, row) => sum + row.bytes, 0) ||
    data.files.reduce((sum, row) => sum + row.bytes, 0) > data.total_bytes) {
    throw new Error('Invalid asset traffic')
  }
  return data
}

export function formatBytes(value: number): string {
  const index = value > 0 ? Math.min(4, Math.floor(Math.log(value) / Math.log(1024))) : 0
  return `${(value / 1024 ** Math.max(0, index)).toLocaleString('ko-KR', { maximumFractionDigits: index > 0 ? 2 : 0 })} ${['B', 'KiB', 'MiB', 'GiB', 'TiB'][Math.max(0, index)]}`
}

export const formatRate = (value: number) => `${formatBytes(value)}/s`
export const perAccountRate = (sample: NetworkHistory['latest']) => sample && sample.accounts > 0
  ? (sample.rx_bytes + sample.tx_bytes) / sample.seconds / sample.accounts : null
