import { describe, expect, it } from 'vitest'
import { render } from 'svelte/server'
import ConcurrentChart from './ConcurrentChart.svelte'

describe('concurrent chart presentation', () => {
  const sample = { timestamp: 125, accounts: 0, web_accounts: 0, agent_accounts: 0, other_accounts: 0,
    peak_accounts: 42, peak_timestamp: 120, sample_count: 1 }
  const current = { timestamp: 86400, accounts: 0, web_accounts: 0, agent_accounts: 0, other_accounts: 0 }

  it('shows minute observations and the period peak for the recent day', () => {
    const history = { from: 0, until: 86400, sample_interval_seconds: 60, current, samples: [sample] }
    const { body } = render(ConcurrentChart, { props: { history, peak: 42 } })
    expect(body).toContain('접속 계정 수 그래프')
    expect(body).toContain('기간 최고 접속 42')
    expect(body).not.toContain('평균 합계')
  })

  it('shows only averages without a peak reference on longer ranges', () => {
    const history = { from: 0, until: 168 * 3600, sample_interval_seconds: 3600,
      current: { ...current, timestamp: 168 * 3600 },
      samples: [{ ...sample, timestamp: 0, accounts: 21, web_accounts: 21, sample_count: 2 }] }
    const { body } = render(ConcurrentChart, { props: { history, peak: 42 } })
    expect(body).toContain('평균 접속 계정 수 그래프')
    expect(body).toContain('평균 합계')
    expect(body).not.toContain('기간 최고 접속')
    expect(body).not.toContain('peak-line')
  })
})
