<script lang="ts" generics="M extends LeaderboardMetric">
  import MetricsError from './MetricsError.svelte'
  import GoldAmount from './GoldAmount.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { createChartSelection } from './chartSelection.svelte'
  import { axisRange, formatAxisTime, formatDateTime, leaderboardMetrics, leaderboardPeriods, type LeaderboardHours, type CharacterLeaderboard, type LeaderboardMetric } from './metrics'
  import { sampleAt, stepChangesAt, stepPath, type StepChange } from './leaderboardHistory'

  let { metric, hours = $bindable(), leaderboard, colors, selectedCharacter = $bindable(), loading, refreshing, error, refresh }: {
    metric: M
    hours: LeaderboardHours
    leaderboard: CharacterLeaderboard<M> | null
    colors: Record<string, string>
    selectedCharacter: string | null
    loading: boolean
    refreshing: boolean
    error: string
    refresh: () => void
  } = $props()
  let { label, axisLabel, enchantPrefix } = $derived(leaderboardMetrics[metric])
  let titleId = $derived(`${metric}-history-title`)
  let container = $state<HTMLDivElement>()
  let width = $state(600)
  let height = $state(360)
  let changes = $state<StepChange[]>([])
  const selection = createChartSelection(timeAtPointer, () => leaderboard?.timestamp ?? null, () => `${hours}:${metric}`, () => { changes = [] })
  let period = $derived(leaderboardPeriods.find((option) => option.hours === hours)!)
  let series = $derived(leaderboard?.series ?? [])
  let focused = $derived(series.some((entry) => entry.name === selectedCharacter) ? selectedCharacter : null)
  let selected = $derived(leaderboard && selection.time !== null && selection.time >= leaderboard.from && selection.time <= leaderboard.timestamp ? selection.time : null)
  let activeChanges = $derived(selected === null ? [] : changes.filter((change) => change.timestamp === selected && series.some((entry) => entry.name === change.name)))
  let highlighted = $derived(new Set(activeChanges.length ? activeChanges.map((change) => change.name) : focused ? [focused] : []))
  let markerSeries = $derived(highlighted.size ? series.filter((entry) => highlighted.has(entry.name)) : series)
  let tooltipSeries = $derived(highlighted.size || series.length <= 10 ? markerSeries : [])
  let left = $derived(metric === 'gold' ? 64 : 42)
  const right = 12
  const top = 20
  const bottom = 38
  let plotWidth = $derived(Math.max(1, width - left - right))
  let plotHeight = $derived(height - top - bottom)
  let { minimum, maximum } = $derived.by(() => {
    let minimum = Infinity
    let maximum = -Infinity
    for (const entry of series) {
      for (const sample of entry.samples) {
        minimum = Math.min(minimum, sample[metric])
        maximum = Math.max(maximum, sample[metric])
      }
    }
    return minimum === Infinity ? { minimum: 1, maximum: 1 } : { minimum, maximum }
  })
  let padding = $derived(metric === 'gold' ? Math.max(1, maximum * .05) : 1)
  let { floor, ceiling, ticks } = $derived(axisRange(minimum, maximum, padding, maximum - minimum + (metric === 'gold' ? padding * 2 : 0)))
  const x = (timestamp: number) => left + (timestamp - (leaderboard?.from ?? 0)) / (hours * 3600) * plotWidth
  const y = (value: number) => top + plotHeight * (1 - (value - floor) / (ceiling - floor))
  let tooltipLeft = $derived(selected === null ? 0 : Math.max(8, Math.min(width - 276, x(selected) - 132)))

  $effect(() => {
    void selectedCharacter
    changes = []
  })

  function timeAtPointer(event: MouseEvent) {
    if (!leaderboard || !container) return null
    const rect = container.getBoundingClientRect()
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top }
    changes = metric === 'weapon_enchant' && point.x >= left && point.x <= width - right && point.y >= top && point.y <= height - bottom
      ? stepChangesAt(series, point, x, y, (sample) => sample[metric]) : []
    const fraction = Math.max(0, Math.min(1, (point.x - left) / plotWidth))
    return changes[0]?.timestamp ?? Math.round(leaderboard.from + fraction * hours * 3600)
  }
</script>

<section class="chart-panel" aria-labelledby={titleId} aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id={titleId}>{label} 변화</h2>
      {#if metric === 'land_plots'}<p>현재 상위 10명의 보유량 · 시간별 관측값</p>{/if}
      {#if metric === 'weapon_enchant'}<p>현재 +7 이상 무기 보유 캐릭터 전체 · 시간별 관측값<br />세로선에 마우스를 올리면 캐릭터와 변화가 표시됩니다. 클릭하면 고정됩니다.</p>{/if}
    </div>
    <PeriodFilter bind:hours options={leaderboardPeriods} label={`${label} 변화 조회 기간`} />
  </div>
  <MetricsError {error} until={leaderboard?.timestamp} {refreshing} {refresh} />
  <div class="chart-meta"><span>{axisLabel}</span></div>
  {#if leaderboard && series.length > 0}
    <div class="chart-canvas" style:min-height={width < 450 ? '280px' : '360px'} bind:this={container}
      bind:contentRect={null, (rect: DOMRectReadOnly | null | undefined) => { if (rect) { width = rect.width; height = rect.height } }}>
      <svg viewBox={`0 0 ${width} ${height}`} role="button" tabindex="0" aria-pressed={selection.pinned} aria-label={`최근 ${period.label} ${metric === 'weapon_enchant' ? '+7 이상 무기 보유' : '상위'} ${series.length}명 ${label} 변화. 캐릭터별 색상은 표와 같습니다.${metric === 'weapon_enchant' ? ' 세로선에 마우스를 올리면 캐릭터와 변화가 표시됩니다.' : ''} 클릭 또는 Enter로 시점 고정, 다시 클릭 또는 Esc로 해제.`}
        {...selection.handlers}>
        {#each ticks as tick (tick)}
          <line x1={left} x2={width - right} y1={y(tick)} y2={y(tick)} class="grid-line" />
          <text x={left - 10} y={y(tick) + 4} text-anchor="end" class="axis-label">{#if metric === 'gold'}<GoldAmount copper={tick} svg />{:else}{enchantPrefix}{tick}{/if}</text>
        {/each}
        {#each width < 500 ? [0, 3, 6] : [0, 2, 4, 6] as tick (tick)}
          <text x={left + plotWidth * tick / 6} y={height - 10} text-anchor={tick === 0 ? 'start' : tick === 6 ? 'end' : 'middle'} class="axis-label">
            {formatAxisTime(leaderboard.from + hours * 3600 * tick / 6, hours)}
          </text>
        {/each}
        {#each [...series].sort((a, b) => Number(highlighted.has(a.name)) - Number(highlighted.has(b.name))) as entry (entry.name)}
          {@const last = entry.samples[entry.samples.length - 1]}
          <g opacity={highlighted.size && !highlighted.has(entry.name) ? 0.16 : 1}>
            <path d={stepPath(entry.samples, leaderboard.timestamp, x, y, (sample) => sample[metric])} fill="none" stroke={colors[entry.name]} stroke-width={highlighted.has(entry.name) ? 3 : 2} stroke-linejoin="round" />
            <circle cx={x(leaderboard.timestamp)} cy={y(last[metric])} r={highlighted.has(entry.name) ? 4 : 3} fill={colors[entry.name]} />
          </g>
        {/each}
        {#if selected !== null}
          <line x1={x(selected)} x2={x(selected)} y1={top} y2={height - bottom} stroke="#83b7ac" stroke-dasharray="4 4" />
          {#each markerSeries as entry (entry.name)}
            {@const sample = sampleAt(entry.samples, selected)}
            {#if sample}
              <circle cx={x(selected)} cy={y(sample[metric])} r="4" fill={colors[entry.name]} stroke="white" stroke-width="2" />
            {/if}
          {/each}
        {/if}
      </svg>
      {#if selected !== null}
        <div class="chart-tooltip" style:left={`${tooltipLeft}px`}>
          <span>{formatDateTime(selected)} KST</span>
          {#if activeChanges.length}
            {#each activeChanges as change (change.name)}
              <div class="tooltip-entry"><span><i style:background={colors[change.name]}></i>{change.name}</span><b>+{change.before} → +{change.after}</b></div>
            {/each}
          {:else}
            {#if tooltipSeries.length === 0}<span>세로선에 마우스를 올리거나 표·범례에서 캐릭터를 선택해 주세요.</span>{/if}
            {#each tooltipSeries as entry (entry.name)}
              {@const sample = sampleAt(entry.samples, selected)}
              <div class="tooltip-entry"><span><i style:background={colors[entry.name]}></i>{entry.name}</span><b>{#if !sample}기록 없음{:else if metric === 'gold'}<GoldAmount copper={sample[metric]} />{:else if metric === 'land_plots'}{sample[metric].toLocaleString('ko-KR')} 필지{:else}{enchantPrefix || 'Lv. '}{sample[metric]}{/if}</b></div>
            {/each}
          {/if}
          <span>{selection.hint}</span>
        </div>
      {/if}
    </div>
    <div class="character-legend" class:scrollable={metric === 'weapon_enchant'} aria-label="캐릭터 선택">
      {#each series as entry (entry.name)}
        <button class:muted={focused !== null && focused !== entry.name} class:active={focused === entry.name} aria-pressed={focused === entry.name}
          onclick={() => { selectedCharacter = focused === entry.name ? null : entry.name }}>
          <i style:background={colors[entry.name]}></i>{entry.name}
        </button>
      {/each}
    </div>
  {:else}
    <div class="chart-empty" role="status">
      <strong>{loading ? `${label} 기록을 불러오고 있어요` : error ? '기록에 연결할 수 없어요' : `아직 ${label} 기록이 없어요`}</strong>
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 그래프가 자동으로 갱신됩니다.' : metric === 'weapon_enchant' ? '+7 이상 무기를 보유하면 변화 기록이 표시됩니다.' : metric === 'land_plots' ? '영지를 보유한 캐릭터가 생기면 보유량 변화가 표시됩니다.' : `캐릭터가 생성되면 ${label} 변화가 기록됩니다.`}</p>
    </div>
  {/if}
</section>

<style>
  .chart-heading { flex-wrap: wrap; gap: 12px; }
  .chart-canvas { flex: 1; }
  .chart-canvas > svg { position: absolute; inset: 0; height: 100%; }
  .character-legend { display: flex; flex-wrap: wrap; gap: 5px; margin: 10px 0; }
  .character-legend.scrollable { max-height: 130px; overflow: auto; }
  .character-legend button { display: flex; align-items: center; gap: 6px; border: 1px solid transparent; border-radius: 6px; padding: 5px 7px; background: #f6f8f7; font-size: 10px; overflow-wrap: anywhere; text-align: left; }
  .character-legend .active { border-color: #bfd6c9; background: #edf5f0; }
  .character-legend .muted { opacity: .5; }
  i { display: inline-block; width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .chart-tooltip { width: 260px; max-width: calc(100% - 16px); }
  .tooltip-entry { display: flex; justify-content: space-between; gap: 12px; font-size: 10px; }
  .tooltip-entry > span { display: flex; align-items: center; gap: 6px; min-width: 0; overflow-wrap: anywhere; }
  .tooltip-entry b { white-space: nowrap; font-weight: 600; font-variant-numeric: tabular-nums; }
</style>
