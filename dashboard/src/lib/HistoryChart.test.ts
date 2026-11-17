import { describe, expect, it } from 'vitest'
import { render } from 'svelte/server'
import HistoryChart from './HistoryChart.svelte'

describe('history chart target line', () => {
  const samples = [0, 1, 2, 3].map((hour) => ({ timestamp: hour * 3600, amount: 100 + hour, target: hour === 0 ? null : 200 + hour }))
  const history = { from: 0, until: 3 * 3600, sample_interval_seconds: 3600, samples }
  const props = { history, peak: null, value: (sample: typeof samples[number]) => sample.amount, legend: '값', valueLabel: '값', fitAxis: true }

  it('draws the target over the samples that carry one and fits the axis to it', () => {
    const { body } = render(HistoryChart, { props: { ...props, target: (sample: typeof samples[number]) => sample.target, targetLabel: '목표 경로' } as never })
    const path = body.match(/<path d="([^"]+)" fill="none" class="target-line"/)![1]
    expect(path.split('L')).toHaveLength(3)
    expect(body).toContain('목표 경로')
  })

  it('omits the target line and legend without target values', () => {
    for (const target of [undefined, () => null]) {
      const { body } = render(HistoryChart, { props: { ...props, target } as never })
      expect(body).not.toContain('target-line')
    }
  })
})
