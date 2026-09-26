import { describe, expect, it } from 'vitest'
import { render } from 'svelte/server'
import HardwarePanel from './HardwarePanel.svelte'
import { parseHardwareStatus, type HardwareStatus } from './hardware'

const fixture = (): HardwareStatus => ({
  until: 1000,
  sample_interval_seconds: 60,
  available: true,
  latest: {
    timestamp: 990,
    cpu_count: 4,
    cpu_percent: 37.5,
    load_average: [1.5, 1.25, 1],
    memory_total_bytes: 8 * 1024 ** 3,
    memory_used_bytes: 3 * 1024 ** 3,
    disks: [{ mount: '/', total_bytes: 100 * 1024 ** 3, available_bytes: 20 * 1024 ** 3 }],
    agent: { instances: 1, process_count: 3, cpu_percent: 12.5, memory_bytes: 256 * 1024 ** 2 },
  },
})

const panel = (history: HardwareStatus, error = '') => render(HardwarePanel, {
  props: { resource: { history, error, loading: false, refreshing: false, refresh() {} } },
}).body

describe('hardware metrics', () => {
  it('displays all requested metrics with units', () => {
    const data = fixture()
    expect(parseHardwareStatus(data)).toEqual(data)
    const html = panel(data)
    for (const text of ['37.5%', '12.5%', '3 GiB / 8 GiB', '256 MiB', '20 GiB', '100 GiB', '1분', '5분', '15분']) {
      expect(html).toContain(text)
    }
    expect(html).toContain('자식 포함 3개 프로세스')
  })

  it('distinguishes warming-up CPU from zero usage and allows unsupported load averages', () => {
    const data = fixture()
    data.latest!.cpu_percent = null
    data.latest!.agent.cpu_percent = null
    data.latest!.load_average = null
    expect(parseHardwareStatus(data)).toEqual(data)
    expect(panel(data)).toContain('CPU 사용률은 첫 수집 후 1분부터 표시합니다')
    expect(panel(data)).not.toContain('Load average')
  })

  it('reports a missing agent instead of implying a measured idle process', () => {
    const data = fixture()
    data.latest!.agent = { instances: 0, process_count: 0, cpu_percent: 0, memory_bytes: 0 }
    expect(parseHardwareStatus(data)).toEqual(data)
    expect(panel(data)).toContain('실행 중인 agent-client가 감지되지 않았습니다')
    expect(panel(data)).not.toContain('>0 B<')
  })

  it('preserves and labels stale or failed observations', () => {
    const data = fixture()
    data.available = false
    expect(parseHardwareStatus(data)).toEqual(data)
    const html = panel(data, '서버 하드웨어 상태를 불러오지 못했어요.')
    expect(html).toContain('마지막으로 수집한 값을 표시합니다')
    expect(html).toContain('서버 하드웨어 상태를 불러오지 못했어요')
    expect(html).toContain('20 GiB')
  })

  it('handles collection not started and missing disks', () => {
    const data = fixture()
    data.latest!.disks = []
    expect(parseHardwareStatus(data)).toEqual(data)
    expect(panel(data)).toContain('조회 가능한 디스크 정보가 없습니다')
    data.latest = null
    data.available = false
    expect(parseHardwareStatus(data)).toEqual(data)
    expect(panel(data)).toContain('하드웨어 정보를 아직 수집하지 못했습니다')
  })

  it('rejects inconsistent counters, missing fields, and nonfinite measurements', () => {
    const invalid: unknown[] = [null, {}, { ...fixture(), sample_interval_seconds: 0 }, { ...fixture(), latest: null }]
    const mutate = (change: (data: NonNullable<HardwareStatus['latest']>) => void) => {
      const data = fixture()
      change(data.latest!)
      invalid.push(data)
    }
    mutate((sample) => { sample.cpu_percent = 101 })
    mutate((sample) => { sample.cpu_count = 0 })
    mutate((sample) => { sample.memory_used_bytes = sample.memory_total_bytes + 1 })
    mutate((sample) => { sample.disks[0].available_bytes = sample.disks[0].total_bytes + 1 })
    mutate((sample) => { sample.agent.cpu_percent = Infinity })
    mutate((sample) => { sample.agent.instances = 0 })
    mutate((sample) => { sample.load_average = [0, NaN, 0] })
    for (const data of invalid) expect(() => parseHardwareStatus(data)).toThrow('Invalid hardware status')
  })
})
