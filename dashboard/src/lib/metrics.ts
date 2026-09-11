export const connectionKinds = [
  { key: 'web_accounts', label: '웹 접속', color: '#168878' },
  { key: 'agent_accounts', label: '외부 에이전트', color: '#7870cf' },
  { key: 'other_accounts', label: '기타·미분류', color: '#9aa6af' },
] as const

export type ConnectionCounts = Record<typeof connectionKinds[number]['key'], number>

export interface TimestampSample {
  timestamp: number
}

export const leaderboardMetrics = {
  level: { label: '레벨', columnLabel: '레벨', axisLabel: '레벨 (Lv.)', description: '', enchantPrefix: '' },
  gold: { label: '골드', columnLabel: '골드', axisLabel: '보유금액', description: '', enchantPrefix: '' },
  weapon_enchant: { label: '무기 인챈트', columnLabel: '인챈트', axisLabel: '인챈트 단계', description: '가방·장비의 무기 중 최고 인챈트', enchantPrefix: '+' },
  armor_enchant: { label: '방어구 인챈트', columnLabel: '합계', axisLabel: '인챈트 합계', description: '가방·장비의 방어구 슬롯별 최고 인챈트 합계', enchantPrefix: '+' },
} as const
export type LeaderboardMetric = keyof typeof leaderboardMetrics
export type CharacterSample<M extends LeaderboardMetric> = TimestampSample & Record<M, number>

export interface CharacterLeaderboard<M extends LeaderboardMetric> extends TimestampSample {
  from: number
  sample_interval_seconds: number
  entries: ({ name: string, account_first_rank: number } & Record<M, number>)[]
  series: { name: string, started_at: number, samples: CharacterSample<M>[] }[]
}

export type LevelLeaderboard = CharacterLeaderboard<'level'>
export type GoldLeaderboard = CharacterLeaderboard<'gold'>
export type WeaponEnchantLeaderboard = CharacterLeaderboard<'weapon_enchant'>
export type ArmorEnchantLeaderboard = CharacterLeaderboard<'armor_enchant'>

export const leaderboardPeriods = [
  { hours: 168, label: '1주일', interval: 3600 },
  { hours: 720, label: '1개월', interval: 3600 },
  { hours: 4320, label: '6개월', interval: 21600 },
  { hours: 8760, label: '1년', interval: 86400 },
] as const
export type LeaderboardHours = typeof leaderboardPeriods[number]['hours']

export const parseLevelLeaderboard = (value: unknown, hours: LeaderboardHours): LevelLeaderboard => parseLeaderboard(value, hours, 'level')
export const parseGoldLeaderboard = (value: unknown, hours: LeaderboardHours): GoldLeaderboard => parseLeaderboard(value, hours, 'gold')
export const parseWeaponEnchantLeaderboard = (value: unknown, hours: LeaderboardHours): WeaponEnchantLeaderboard => parseLeaderboard(value, hours, 'weapon_enchant')
export const parseArmorEnchantLeaderboard = (value: unknown, hours: LeaderboardHours): ArmorEnchantLeaderboard => parseLeaderboard(value, hours, 'armor_enchant')

function parseLeaderboard<M extends LeaderboardMetric>(value: unknown, hours: LeaderboardHours, metric: M): CharacterLeaderboard<M> {
  if (!value || typeof value !== 'object') throw new Error(`Invalid ${metric} leaderboard response`)
  const data = value as CharacterLeaderboard<M>
  const minimum = metric === 'level' ? 1 : 0
  const interval = leaderboardPeriods.find((period) => period.hours === hours)!.interval
  if (!Number.isSafeInteger(data.timestamp) || data.timestamp < 0 ||
    !Number.isSafeInteger(data.from) || data.timestamp - data.from !== hours * 3600 ||
    data.sample_interval_seconds !== interval ||
    !Array.isArray(data.entries) || data.entries.length > 10 ||
    !data.entries.every((entry, index) => entry && typeof entry.name === 'string' && entry.name.trim().length > 0 &&
      Number.isSafeInteger(entry[metric]) && entry[metric] >= minimum &&
      Number.isSafeInteger(entry.account_first_rank) && entry.account_first_rank >= 1 && entry.account_first_rank <= index + 1 &&
      data.entries[entry.account_first_rank - 1]?.account_first_rank === entry.account_first_rank &&
      (index === 0 || entry[metric] <= data.entries[index - 1][metric])) ||
    new Set(data.entries.map((entry) => entry.name)).size !== data.entries.length) {
    throw new Error(`Invalid ${metric} leaderboard response`)
  }
  if (!Array.isArray(data.series) || data.series.length !== data.entries.length ||
    !data.series.every((series, index) => series && series.name === data.entries[index].name &&
      Number.isSafeInteger(series.started_at) && series.started_at >= 0 && series.started_at <= data.timestamp &&
      Array.isArray(series.samples) && series.samples.length > 0 && series.samples.length <= Math.ceil(hours * 3600 / interval) + 3 &&
      series.samples[0].timestamp === Math.max(data.from, series.started_at) &&
      series.samples.every((sample, sampleIndex) => sample && Number.isSafeInteger(sample.timestamp) &&
        sample.timestamp >= data.from && sample.timestamp <= data.timestamp &&
        Number.isSafeInteger(sample[metric]) && sample[metric] >= minimum &&
        (sampleIndex === 0 || sample.timestamp > series.samples[sampleIndex - 1].timestamp)))) {
    throw new Error(`Invalid ${metric} history response`)
  }
  return data
}

export interface AccountSample extends TimestampSample {
  accounts: number
}

export interface Sample extends AccountSample, ConnectionCounts {}

export interface HistorySample extends Sample {
  peak_accounts: number
  peak_timestamp: number
  sample_count: number
}

export interface ChartHistory<T extends TimestampSample = AccountSample> {
  from: number
  until: number
  sample_interval_seconds: number
  samples: T[]
}

export interface ConcurrentHistory extends ChartHistory<HistorySample> {
  current: Sample
}

export interface UniqueHistory extends ChartHistory {
  window_seconds: number
  collection_started_at: number
  last_aggregated_at: number | null
}

export interface GoldSample extends TimestampSample {
  total_gold: number
}

export interface GoldHistorySample extends GoldSample {
  peak_gold: number
  sample_count: number
}

export interface GoldHistory extends ChartHistory<GoldHistorySample> {
  latest: GoldSample | null
}

export interface PerAccountGoldSample extends GoldSample {
  accounts: number
  gold_per_account: number
}

export interface PerAccountGoldHistorySample extends TimestampSample {
  gold_per_account: number
  peak_gold_per_account: number
  sample_count: number
}

export interface PerAccountGoldHistory extends ChartHistory<PerAccountGoldHistorySample> {
  window_seconds: number
  collection_started_at: number
  latest: PerAccountGoldSample | null
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
export const goldPeriods = [
  { hours: 1, label: '1시간', interval: 3600, intervalLabel: '1시간 간격' },
  { hours: 24, label: '24시간', interval: 3600, intervalLabel: '1시간 간격' },
  { hours: 168, label: '1주일', interval: 3600, intervalLabel: '1시간 간격' },
  { hours: 720, label: '1개월', interval: 3600, intervalLabel: '1시간 간격' },
  { hours: 4320, label: '6개월', interval: 21600, intervalLabel: '6시간 평균' },
  { hours: 8760, label: '1년', interval: 86400, intervalLabel: '1일 평균' },
] as const
export type GoldHours = typeof goldPeriods[number]['hours']
export const uniquePeriods = [
  { hours: 24, label: '1일' },
  { hours: 168, label: '1주일' },
  { hours: 720, label: '1개월' },
  { hours: 4320, label: '6개월' },
  { hours: 8760, label: '1년' },
] as const
export type UniqueHours = typeof uniquePeriods[number]['hours']
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

export function parseUniqueHistory(value: unknown, hours: UniqueHours): UniqueHistory {
  if (!value || typeof value !== 'object') throw new Error('Invalid unique metrics response')
  const data = value as UniqueHistory
  const day = 86400
  const last = data.last_aggregated_at
  if (!Number.isSafeInteger(data.from) || !Number.isSafeInteger(data.until) ||
    data.until - data.from !== hours * 3600 || data.window_seconds !== hours * 3600 ||
    (data.until + 9 * 3600) % day !== 0 || data.sample_interval_seconds !== day ||
    !Number.isSafeInteger(data.collection_started_at) || data.collection_started_at < 0 ||
    (last !== null && (!Number.isSafeInteger(last) || last > data.until || last <= data.collection_started_at || (last + 9 * 3600) % day !== 0)) ||
    !Array.isArray(data.samples) || data.samples.length > hours / 24 + 1 ||
    !data.samples.every((sample, index) => sample && Number.isSafeInteger(sample.timestamp) &&
      Number.isSafeInteger(sample.accounts) && sample.accounts >= 0 &&
      sample.timestamp >= data.from && sample.timestamp > data.collection_started_at && sample.timestamp <= data.until &&
      (sample.timestamp + 9 * 3600) % day === 0 &&
      (index === 0 || sample.timestamp > data.samples[index - 1].timestamp)) ||
    (last !== null && last >= data.from ? data.samples.at(-1)?.timestamp !== last : data.samples.length !== 0)) {
    throw new Error('Invalid unique metrics response')
  }
  return data
}

function isGoldSample(value: unknown, until: number): value is GoldSample {
  if (!value || typeof value !== 'object') return false
  const sample = value as GoldSample
  return Number.isSafeInteger(sample.timestamp) && sample.timestamp >= 0 &&
    sample.timestamp <= until && sample.timestamp % 3600 === 0 &&
    Number.isSafeInteger(sample.total_gold) && sample.total_gold >= 0
}

function hasValidGoldSamples<T extends TimestampSample & { sample_count: number }>(
  data: ChartHistory<T>, hours: GoldHours, validValue: (sample: T) => boolean,
) {
  const interval = data.sample_interval_seconds
  return Array.isArray(data.samples) && data.samples.length <= Math.ceil(hours * 3600 / interval) + 1 &&
    data.samples.every((sample, index) => sample && Number.isSafeInteger(sample.timestamp) &&
      sample.timestamp >= data.from && sample.timestamp <= data.until &&
      (sample.timestamp === data.from || sample.timestamp % interval === 0) && validValue(sample) &&
      Number.isSafeInteger(sample.sample_count) && sample.sample_count > 0 && sample.sample_count <= interval / 3600 &&
      (index === 0 || Math.floor(sample.timestamp / interval) > Math.floor(data.samples[index - 1].timestamp / interval)))
}

export function parseGoldHistory(value: unknown, hours: GoldHours): GoldHistory {
  if (!value || typeof value !== 'object') throw new Error('Invalid gold metrics response')
  const data = value as GoldHistory
  const interval = goldPeriods.find((period) => period.hours === hours)!.interval
  const latest = data.latest
  if (!Number.isSafeInteger(data.from) || !Number.isSafeInteger(data.until) ||
    data.until - data.from !== hours * 3600 || data.sample_interval_seconds !== interval ||
    (latest !== null && !isGoldSample(latest, data.until)) ||
    !hasValidGoldSamples(data, hours, (sample) =>
      Number.isFinite(sample.total_gold) && sample.total_gold >= 0 &&
      Number.isSafeInteger(sample.peak_gold) && sample.peak_gold >= sample.total_gold) ||
    (latest !== null && latest.timestamp >= data.from
      ? data.samples.at(-1)?.timestamp !== Math.max(data.from, Math.floor(latest.timestamp / interval) * interval)
      : data.samples.length !== 0)) {
    throw new Error('Invalid gold metrics response')
  }
  return data
}

export function parsePerAccountGoldHistory(value: unknown, hours: GoldHours, activeHours: UniqueHours): PerAccountGoldHistory {
  if (!value || typeof value !== 'object') throw new Error('Invalid per-account gold metrics response')
  const data = value as PerAccountGoldHistory
  const interval = goldPeriods.find((period) => period.hours === hours)!.interval
  const latest = data.latest
  if (!Number.isSafeInteger(data.from) || !Number.isSafeInteger(data.until) ||
    data.until - data.from !== hours * 3600 || data.sample_interval_seconds !== interval ||
    data.window_seconds !== activeHours * 3600 ||
    !Number.isSafeInteger(data.collection_started_at) || data.collection_started_at < 0 ||
    (latest !== null && (!isGoldSample(latest, data.until) ||
      kstDayStart(latest.timestamp) <= data.collection_started_at ||
      !Number.isSafeInteger(latest.accounts) || latest.accounts <= 0 ||
      !Number.isFinite(latest.gold_per_account) || latest.gold_per_account < 0 ||
      Math.abs(latest.gold_per_account - latest.total_gold / latest.accounts) > Math.max(1, latest.gold_per_account) * 1e-9)) ||
    !hasValidGoldSamples(data, hours, (sample) =>
      Number.isFinite(sample.gold_per_account) && sample.gold_per_account >= 0 &&
      Number.isFinite(sample.peak_gold_per_account) && sample.peak_gold_per_account >= 0 &&
      sample.gold_per_account - sample.peak_gold_per_account <= Math.max(1, sample.peak_gold_per_account) * 1e-9) ||
    (latest !== null && (latest.timestamp >= data.from
      ? data.samples.at(-1)?.timestamp !== Math.max(data.from, Math.floor(latest.timestamp / interval) * interval)
      : data.samples.length !== 0))) {
    throw new Error('Invalid per-account gold metrics response')
  }
  return data
}

export const kstDayStart = (timestamp: number) => Math.floor((timestamp + 9 * 3600) / 86400) * 86400 - 9 * 3600

export function axisStep(span: number) {
  const raw = Math.max(1, span / 4)
  const magnitude = 10 ** Math.floor(Math.log10(raw))
  return ([1, 2, 5, 10].find((value) => value * magnitude >= raw) ?? 10) * magnitude
}

export function splitSegments<T extends TimestampSample>(samples: T[], interval: number): T[][] {
  const segments: T[][] = []
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

export function nearestSample(samples: TimestampSample[], timestamp: number): number | null {
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
  timeZone: 'Asia/Seoul', month: 'numeric', day: 'numeric', weekday: 'short',
})
const monthFormatter = new Intl.DateTimeFormat('ko-KR', {
  timeZone: 'Asia/Seoul', year: '2-digit', month: 'numeric',
})

export const formatTime = (timestamp: number) => timeFormatter.format(timestamp * 1000)
export const formatCount = (value: number) => value.toLocaleString('ko-KR', { maximumFractionDigits: 1 })
export function goldSegments(copper: number) {
  const amount = Math.round(copper)
  return [
    { unit: 'gold', value: Math.floor(amount / 10_000), suffix: 'g' },
    { unit: 'silver', value: Math.floor(amount % 10_000 / 100), suffix: 's' },
    { unit: 'copper', value: amount % 100, suffix: 'c' },
  ].filter((part) => part.value > 0 || (amount === 0 && part.unit === 'copper'))
    .map((part) => ({ unit: part.unit, text: `${part.value.toLocaleString('ko-KR')}${part.suffix}` }))
}
export const formatGold = (copper: number) => goldSegments(copper).map((part) => part.text).join('')
export const formatDateTime = (timestamp: number) => dateFormatter.format(timestamp * 1000)
export const formatAxisTime = (timestamp: number, hours: number, daily = false) =>
  (hours <= 24 && !daily ? timeFormatter : hours <= 4320 ? dayFormatter : monthFormatter).format(timestamp * 1000)
