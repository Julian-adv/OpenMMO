<script lang="ts" generics="M extends LeaderboardMetric">
  import MetricsError from './MetricsError.svelte'
  import GoldAmount from './GoldAmount.svelte'
  import PeriodFilter from './PeriodFilter.svelte'
  import { formatAxisTime, formatDateTime, leaderboardMetrics, leaderboardPeriods, type LeaderboardHours, type CharacterLeaderboard, type LeaderboardMetric } from './metrics'
  import { axisRange, sampleAt, stepPath } from './leaderboardHistory'

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
  let selectedTime = $state<number | null>(null)
  let period = $derived(leaderboardPeriods.find((option) => option.hours === hours)!)
  let series = $derived(leaderboard?.series ?? [])
  let focused = $derived(series.some((entry) => entry.name === selectedCharacter) ? selectedCharacter : null)
  let left = $derived(metric === 'gold' ? 64 : 42)
  const right = 12
  const top = 20
  const bottom = 38
  let plotWidth = $derived(Math.max(1, width - left - right))
  let plotHeight = $derived(height - top - bottom)
  let values = $derived(series.flatMap((entry) => entry.samples.map((sample) => sample[metric])))
  let minimum = $derived(values.length ? Math.min(...values) : 1)
  let maximum = $derived(values.length ? Math.max(...values) : 1)
  let padding = $derived(metric === 'gold' ? Math.max(1, maximum * .05) : 1)
  let { floor, ceiling, ticks } = $derived(axisRange(minimum, maximum, padding, maximum - minimum + (metric === 'gold' ? padding * 2 : 0)))
  const x = (timestamp: number) => left + (timestamp - (leaderboard?.from ?? 0)) / (hours * 3600) * plotWidth
  const y = (value: number) => top + plotHeight * (1 - (value - floor) / (ceiling - floor))
  let selected = $derived(leaderboard && selectedTime !== null && selectedTime >= leaderboard.from && selectedTime <= leaderboard.timestamp ? selectedTime : null)
  let tooltipLeft = $derived(selected === null ? 0 : Math.max(8, Math.min(width - 276, x(selected) - 132)))

  function selectAtPointer(event: PointerEvent) {
    if (!leaderboard || !container) return
    const fraction = Math.max(0, Math.min(1, (event.clientX - container.getBoundingClientRect().left - left) / plotWidth))
    selectedTime = Math.round(leaderboard.from + fraction * hours * 3600)
  }
</script>

<section class="chart-panel" aria-labelledby={titleId} aria-busy={loading}>
  <div class="chart-heading">
    <div>
      <h2 id={titleId}>{label} 변화</h2>
      {#if metric === 'land_plots'}<p>현재 상위 10명의 보유량 · 시간별 관측값</p>{/if}
    </div>
    <PeriodFilter bind:hours options={leaderboardPeriods} label={`${label} 변화 조회 기간`} />
  </div>
  <MetricsError {error} until={leaderboard?.timestamp} {refreshing} {refresh} />
  <div class="chart-meta"><span>{axisLabel}</span></div>
  {#if leaderboard && series.length > 0}
    <div class="chart-canvas" style:min-height={width < 450 ? '280px' : '360px'} bind:this={container}
      bind:contentRect={null, (rect: DOMRectReadOnly | null | undefined) => { if (rect) { width = rect.width; height = rect.height } }}>
      <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`최근 ${period.label} 상위 ${series.length}명 ${label} 변화. 캐릭터별 색상은 순위 표와 같습니다.`}
        onpointermove={selectAtPointer} onpointerleave={() => { selectedTime = null }}>
        {#each ticks as tick (tick)}
          <line x1={left} x2={width - right} y1={y(tick)} y2={y(tick)} class="grid-line" />
          <text x={left - 10} y={y(tick) + 4} text-anchor="end" class="axis-label">{#if metric === 'gold'}<GoldAmount copper={tick} svg />{:else}{enchantPrefix}{tick}{/if}</text>
        {/each}
        {#each width < 500 ? [0, 3, 6] : [0, 2, 4, 6] as tick (tick)}
          <text x={left + plotWidth * tick / 6} y={height - 10} text-anchor={tick === 0 ? 'start' : tick === 6 ? 'end' : 'middle'} class="axis-label">
            {formatAxisTime(leaderboard.from + hours * 3600 * tick / 6, hours)}
          </text>
        {/each}
        {#each [...series].sort((a, b) => Number(a.name === focused) - Number(b.name === focused)) as entry (entry.name)}
          {@const last = entry.samples[entry.samples.length - 1]}
          <g opacity={focused && focused !== entry.name ? 0.16 : 1}>
            <path d={stepPath(entry.samples, leaderboard.timestamp, x, y, (sample) => sample[metric])} fill="none" stroke={colors[entry.name]} stroke-width={focused === entry.name ? 3 : 2} stroke-linejoin="round" />
            <circle cx={x(leaderboard.timestamp)} cy={y(last[metric])} r={focused === entry.name ? 4 : 3} fill={colors[entry.name]} />
          </g>
        {/each}
        {#if selected !== null}
          <line x1={x(selected)} x2={x(selected)} y1={top} y2={height - bottom} stroke="#83b7ac" stroke-dasharray="4 4" />
          {#each series as entry (entry.name)}
            {@const sample = sampleAt(entry.samples, selected)}
            {#if sample && (!focused || focused === entry.name)}
              <circle cx={x(selected)} cy={y(sample[metric])} r="4" fill={colors[entry.name]} stroke="white" stroke-width="2" />
            {/if}
          {/each}
        {/if}
      </svg>
      {#if selected !== null}
        <div class="chart-tooltip" style:left={`${tooltipLeft}px`}>
          <span>{formatDateTime(selected)} KST</span>
          {#each series.filter((entry) => !focused || focused === entry.name) as entry (entry.name)}
            {@const sample = sampleAt(entry.samples, selected)}
            <div class="tooltip-entry"><span><i style:background={colors[entry.name]}></i>{entry.name}</span><b>{#if !sample}기록 없음{:else if metric === 'gold'}<GoldAmount copper={sample[metric]} />{:else if metric === 'land_plots'}{sample[metric].toLocaleString('ko-KR')} 필지{:else}{enchantPrefix || 'Lv. '}{sample[metric]}{/if}</b></div>
          {/each}
        </div>
      {/if}
    </div>
    <div class="character-legend" aria-label="캐릭터 선택">
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
      <p>{loading ? '잠시만 기다려 주세요.' : error ? '연결이 복구되면 그래프가 자동으로 갱신됩니다.' : metric === 'land_plots' ? '영지를 보유한 캐릭터가 생기면 보유량 변화가 표시됩니다.' : `캐릭터가 생성되면 ${label} 변화가 기록됩니다.`}</p>
    </div>
  {/if}
</section>

<style>
  .chart-heading { flex-wrap: wrap; gap: 12px; }
  .chart-canvas { flex: 1; }
  .chart-canvas > svg { position: absolute; inset: 0; height: 100%; }
  .character-legend { display: flex; flex-wrap: wrap; gap: 5px; margin: 10px 0; }
  .character-legend button { display: flex; align-items: center; gap: 6px; border: 1px solid transparent; border-radius: 6px; padding: 5px 7px; background: #f6f8f7; font-size: 10px; overflow-wrap: anywhere; text-align: left; }
  .character-legend .active { border-color: #bfd6c9; background: #edf5f0; }
  .character-legend .muted { opacity: .5; }
  i { display: inline-block; width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .chart-tooltip { width: 260px; max-width: calc(100% - 16px); }
  .tooltip-entry { display: flex; justify-content: space-between; gap: 12px; font-size: 10px; }
  .tooltip-entry > span { display: flex; align-items: center; gap: 6px; min-width: 0; overflow-wrap: anywhere; }
  .tooltip-entry b { white-space: nowrap; font-weight: 600; font-variant-numeric: tabular-nums; }
</style>
