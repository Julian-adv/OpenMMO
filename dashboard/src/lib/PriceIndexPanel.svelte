<script lang="ts">
  import type { MetricsResource } from './metricsResource.svelte'
  import GoldAmount from './GoldAmount.svelte'
  import MetricsError from './MetricsError.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatAxisTime, formatCount, formatDateTime, leaderboardPeriods, nearestSample, type LeaderboardHours, type PriceIndexHistory } from './metrics'
  import { axisRange, stepPath } from './leaderboardHistory'

  let { hours = $bindable(), resource }: {
    hours: LeaderboardHours
    resource: MetricsResource<PriceIndexHistory>
  } = $props()
  let { history, loading, refreshing, error, refresh } = $derived(resource)
  let period = $derived(leaderboardPeriods.find((option) => option.hours === hours)!)
  let container = $state<HTMLDivElement>()
  let width = $state(600)
  let height = $state(320)
  let selectedTime = $state<number | null>(null)
  const left = 48
  const right = 18
  const top = 24
  const bottom = 38
  let plotWidth = $derived(Math.max(1, width - left - right))
  let plotHeight = $derived(height - top - bottom)
  let meetings = $derived(history?.meetings ?? [])
  let steps = $derived(history ? [{ timestamp: history.from, index: history.baseline_index_percent }, ...meetings.map((meeting) => ({ timestamp: meeting.timestamp, index: meeting.index_after }))] : [])
  let values = $derived([100, ...steps.map((step) => step.index)])
  let minimum = $derived(Math.min(...values))
  let maximum = $derived(Math.max(...values))
  let { floor, ceiling, ticks } = $derived(axisRange(minimum, maximum, 5, maximum - minimum + 10))
  const x = (timestamp: number) => left + (timestamp - (history?.from ?? 0)) / (hours * 3600) * plotWidth
  const y = (value: number) => top + plotHeight * (1 - (value - floor) / (ceiling - floor))
  let selected = $derived(meetings.find((meeting) => meeting.timestamp === selectedTime) ?? null)
  let tooltipLeft = $derived(selected ? Math.max(8, Math.min(width - 244, x(selected.timestamp) - 118)) : 0)
  let last = $derived(meetings.at(-1))
  const signed = (ratio: number) => `${ratio > 0 ? '+' : ''}${formatCount(ratio * 100)}%`

  function selectAtPointer(event: PointerEvent) {
    if (!history || !container) return
    const fraction = Math.max(0, Math.min(1, (event.clientX - container.getBoundingClientRect().left - left) / plotWidth))
    const timestamp = history.from + fraction * hours * 3600
    const index = nearestSample(meetings, timestamp)
    selectedTime = index !== null && Math.abs(meetings[index].timestamp - timestamp) <= hours * 3600 * 16 / plotWidth ? meetings[index].timestamp : null
  }
</script>

<section class="chart-panel" aria-labelledby="price-index-title" aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id="price-index-title">상인 물가 지수</h2>
      <p>진열대 소비재 구매가에 곱하는 지수 · 기본 100%</p>
    </div>
    <PeriodFilter bind:hours options={leaderboardPeriods} label="물가 지수 조회 기간" />
  </div>
  <MetricsError {error} until={history?.until} {refreshing} {refresh} />
  <div class="metric-summary">
    <strong>{history ? `${history.current_index_percent}%` : '—'}</strong>
    <p>{last ? `마지막 회의: ${formatDateTime(last.timestamp)} KST · ${last.index_before}% → ${last.index_after}%` : history ? `최근 ${period.label} 동안 회의 기록이 없어요` : '회의 기록을 확인하고 있어요'}</p>
  </div>
  <div class="chart-meta"><span>물가 지수 (%)</span><span>상인 회의마다 조정</span></div>
  {#if history}
    <div class="chart-canvas" style:min-height={width < 450 ? '260px' : '320px'} bind:this={container}
      bind:contentRect={null, (rect: DOMRectReadOnly | null | undefined) => { if (rect) { width = rect.width; height = rect.height } }}>
      <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`최근 ${period.label} 상인 물가 지수 변화. ${meetings.length}회 회의.`}
        onpointermove={selectAtPointer} onpointerleave={() => { selectedTime = null }}>
        {#each ticks as tick (tick)}
          <line x1={left} x2={width - right} y1={y(tick)} y2={y(tick)} class="grid-line" />
          <text x={left - 10} y={y(tick) + 4} text-anchor="end" class="axis-label">{tick}%</text>
        {/each}
        {#each width < 500 ? [0, 3, 6] : [0, 2, 4, 6] as tick (tick)}
          <text x={left + plotWidth * tick / 6} y={height - 10} text-anchor={tick === 0 ? 'start' : tick === 6 ? 'end' : 'middle'} class="axis-label">
            {formatAxisTime(history.from + hours * 3600 * tick / 6, hours)}
          </text>
        {/each}
        <line x1={left} x2={width - right} y1={y(100)} y2={y(100)} class="peak-line" />
        <path d={stepPath(steps, history.until, x, y, (sample) => sample.index)} fill="none" stroke="#31594f" stroke-width="2.5" stroke-linejoin="round" />
        {#each meetings as meeting (meeting.game_day)}
          <circle cx={x(meeting.timestamp)} cy={y(meeting.index_after)} r={selected === meeting ? 5 : 3.5} fill="#31594f" stroke="white" stroke-width={selected === meeting ? 2.5 : 0} />
        {/each}
        {#if selected}
          <line x1={x(selected.timestamp)} x2={x(selected.timestamp)} y1={top} y2={height - bottom} stroke="#83b7ac" stroke-dasharray="4 4" />
        {/if}
      </svg>
      {#if selected}
        <div class="chart-tooltip" style:left={`${tooltipLeft}px`}>
          <span>{formatDateTime(selected.timestamp)} KST · 게임일 {selected.game_day}</span>
          <strong>{selected.index_before}% → {selected.index_after}%</strong>
          <span>활성 유저 1인당 골드 {signed(selected.growth)} · <GoldAmount copper={selected.m_prev} /> → <GoldAmount copper={selected.m_now} /></span>
        </div>
      {/if}
    </div>
    <div class="chart-legend" aria-label="그래프 범례">
      <span class="legend"><i class="total-line"></i>물가 지수</span>
      <span class="legend"><i></i>상인 회의</span>
      <span class="legend"><i class="peak-line"></i>기본 100%</span>
    </div>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? '물가 지수 기록을 불러오고 있어요' : '기록에 연결할 수 없어요'}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : '연결이 복구되면 그래프가 자동으로 갱신됩니다.'}</p>
    </div>
  {/if}
  <div class="chart-footer">
    <span>{history ? `${formatDateTime(history.from)} — ${formatDateTime(history.until)}` : `최근 ${period.label}`} <span class="timezone">KST</span></span>
    <span>{history ? `${meetings.length.toLocaleString('ko-KR')}회 회의` : '기록 확인 중'}</span>
  </div>
</section>

<style>
  .chart-heading { flex-wrap: wrap; gap: 12px; }
  .chart-canvas { flex: 1; }
  .chart-canvas > svg { position: absolute; inset: 0; height: 100%; }
</style>
