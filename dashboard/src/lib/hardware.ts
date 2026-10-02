import { formatCount, type ChartHistory } from './metrics'

export const hardwarePeriods = [
  { hours: 1, label: '1시간' },
  { hours: 24, label: '24시간' },
  { hours: 168, label: '7일' },
] as const
export type HardwareHours = typeof hardwarePeriods[number]['hours']

export interface ProcessUsage {
  process_count: number
  cpu_percent: number | null
  memory_bytes: number
}

export interface HardwareSnapshot {
  timestamp: number
  cpu_count: number
  cpu_percent: number | null
  load_average: [number, number, number] | null
  memory_total_bytes: number
  memory_used_bytes: number
  disks: { mount: string, total_bytes: number, available_bytes: number }[]
  agent: {
    instances: number
    process_count: number
    cpu_percent: number | null
    memory_bytes: number
    client: ProcessUsage
    llm: ProcessUsage
  }
}

export interface HardwareStatus extends ChartHistory<HardwareSnapshot> {
  retention_seconds: number
  collection_started_at: number | null
  available: boolean
  latest: HardwareSnapshot | null
}

const count = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0
const nonnegative = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value) && value >= 0

function validSnapshot(sample: HardwareSnapshot): boolean {
  if (!sample || !count(sample.timestamp) ||
    !count(sample.cpu_count) || sample.cpu_count === 0 ||
    sample.cpu_percent !== null && (!nonnegative(sample.cpu_percent) || sample.cpu_percent > 100) ||
    sample.load_average !== null && (!Array.isArray(sample.load_average) || sample.load_average.length !== 3 || !sample.load_average.every(nonnegative)) ||
    !count(sample.memory_total_bytes) || sample.memory_total_bytes === 0 ||
    !count(sample.memory_used_bytes) || sample.memory_used_bytes > sample.memory_total_bytes) {
    return false
  }
  if (!Array.isArray(sample.disks) || sample.disks.some((disk) => !disk || typeof disk.mount !== 'string' || disk.mount.length === 0 ||
    !count(disk.total_bytes) || disk.total_bytes === 0 || !count(disk.available_bytes) || disk.available_bytes > disk.total_bytes)) {
    return false
  }

  const agent = sample.agent
  const usage = (value: ProcessUsage) => value && count(value.process_count) && count(value.memory_bytes) &&
    (value.cpu_percent === null || nonnegative(value.cpu_percent)) &&
    (value.process_count > 0 || value.memory_bytes === 0 && (value.cpu_percent === null || value.cpu_percent === 0))
  if (!agent || !count(agent.instances) || !count(agent.process_count) || agent.instances > agent.process_count ||
    agent.cpu_percent !== null && !nonnegative(agent.cpu_percent) || !count(agent.memory_bytes)) {
    return false
  }
  if (!usage(agent.client) || !usage(agent.llm) || agent.instances !== agent.client.process_count ||
    agent.process_count !== agent.client.process_count + agent.llm.process_count ||
    agent.memory_bytes !== agent.client.memory_bytes + agent.llm.memory_bytes ||
    [agent.client, agent.llm].some((group) => (group.cpu_percent === null) !== (agent.cpu_percent === null))) return false
  if (agent.instances === 0 && (agent.process_count !== 0 || agent.memory_bytes !== 0 ||
    agent.cpu_percent !== null && agent.cpu_percent !== 0)) {
    return false
  }
  return true
}

export function parseHardwareStatus(value: unknown, hours: HardwareHours = 24): HardwareStatus {
  if (!value || typeof value !== 'object') throw new Error('Invalid hardware status')
  const data = value as HardwareStatus
  if (!count(data.from) || !count(data.until) || data.until - data.from !== hours * 3600 ||
    data.sample_interval_seconds !== 60 || data.retention_seconds !== 7 * 86400 ||
    typeof data.available !== 'boolean' ||
    !(data.collection_started_at === null || count(data.collection_started_at) &&
      data.collection_started_at > data.until - data.retention_seconds && data.collection_started_at <= data.until) ||
    !(data.latest === null ? !data.available : validSnapshot(data.latest) && data.latest.timestamp <= data.until) ||
    !Array.isArray(data.samples) ||
    !data.samples.every((sample, index) => validSnapshot(sample) && sample.timestamp > data.from && sample.timestamp <= data.until &&
      (index === 0 || sample.timestamp > data.samples[index - 1].timestamp))) {
    throw new Error('Invalid hardware status')
  }
  return data
}

export const formatPercent = (value: number | null | undefined) => value == null
  ? '—' : `${formatCount(value)}%`

export const usagePercent = (used: number, total: number) => used / total * 100
