import { formatCount } from './metrics'

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
  }
}

export interface HardwareStatus {
  until: number
  sample_interval_seconds: number
  available: boolean
  latest: HardwareSnapshot | null
}

const count = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0
const nonnegative = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value) && value >= 0

export function parseHardwareStatus(value: unknown): HardwareStatus {
  if (!value || typeof value !== 'object') throw new Error('Invalid hardware status')
  const data = value as HardwareStatus
  if (!count(data.until) || !count(data.sample_interval_seconds) || data.sample_interval_seconds < 1 ||
    typeof data.available !== 'boolean') {
    throw new Error('Invalid hardware status')
  }

  const sample = data.latest
  if (sample === null) {
    if (data.available) throw new Error('Invalid hardware status')
    return data
  }
  if (!sample || !count(sample.timestamp) || sample.timestamp > data.until ||
    !count(sample.cpu_count) || sample.cpu_count === 0 ||
    sample.cpu_percent !== null && (!nonnegative(sample.cpu_percent) || sample.cpu_percent > 100) ||
    sample.load_average !== null && (!Array.isArray(sample.load_average) || sample.load_average.length !== 3 || !sample.load_average.every(nonnegative)) ||
    !count(sample.memory_total_bytes) || sample.memory_total_bytes === 0 ||
    !count(sample.memory_used_bytes) || sample.memory_used_bytes > sample.memory_total_bytes) {
    throw new Error('Invalid hardware status')
  }
  if (!Array.isArray(sample.disks) || sample.disks.some((disk) => !disk || typeof disk.mount !== 'string' || disk.mount.length === 0 ||
    !count(disk.total_bytes) || disk.total_bytes === 0 || !count(disk.available_bytes) || disk.available_bytes > disk.total_bytes)) {
    throw new Error('Invalid hardware status')
  }

  const agent = sample.agent
  if (!agent || !count(agent.instances) || !count(agent.process_count) || agent.instances > agent.process_count ||
    agent.cpu_percent !== null && !nonnegative(agent.cpu_percent) || !count(agent.memory_bytes)) {
    throw new Error('Invalid hardware status')
  }
  if (agent.instances === 0 && (agent.process_count !== 0 || agent.memory_bytes !== 0 ||
    agent.cpu_percent !== null && agent.cpu_percent !== 0)) {
    throw new Error('Invalid hardware status')
  }
  return data
}

export const formatPercent = (value: number | null | undefined) => value == null
  ? '—' : `${formatCount(value)}%`

export const usagePercent = (used: number, total: number) => used / total * 100
