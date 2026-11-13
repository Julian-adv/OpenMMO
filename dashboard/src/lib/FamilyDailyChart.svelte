<script lang="ts">
  import { axisRange, formatCount, formatDay, formatShortDay } from './metrics'
  import { dailySeries, type FamilyDayCounts } from './families'

  let { days }: { days: FamilyDayCounts[] } = $props()
  let width = $state(920)
  let hovered = $state<number | null>(null)
  const height = 190
  const top = 12
  const bottom = 26
  const left = 30
  let plotWidth = $derived(Math.max(1, width - left - 8))
  let band = $derived(plotWidth / Math.max(1, days.length))
  let bar = $derived(Math.max(1.5, Math.min(9, (band - 4) / dailySeries.length - 2)))
  let { ceiling, ticks } = $derived(axisRange(0, Math.max(...days.flatMap((day) => dailySeries.map((series) => day[series.key]))), 0))
  let labelEvery = $derived(Math.max(1, Math.ceil(52 / band)))
  const y = (value: number) => top + (height - top - bottom) * (1 - value / ceiling)
  const groupX = (index: number) => left + band * index + (band - dailySeries.length * (bar + 2) + 2) / 2

  function barPath(x: number, value: number) {
    const base = y(0)
    const r = Math.min(4, bar / 2, base - y(value))
    return `M${x},${base} V${y(value) + r} Q${x},${y(value)} ${x + r},${y(value)} H${x + bar - r} Q${x + bar},${y(value)} ${x + bar},${y(value) + r} V${base} Z`
  }

  function move(index: number, event: KeyboardEvent) {
    const next = event.key === 'ArrowLeft' ? index - 1 : event.key === 'ArrowRight' ? index + 1 : null
    if (next === null || next < 0 || next >= days.length) return
    event.preventDefault()
    hovered = next
  }
</script>

<div class="chart-canvas" bind:contentRect={null, (rect: DOMRectReadOnly | null | undefined) => { if (rect) width = rect.width }}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <svg viewBox={`0 0 ${width} ${height}`} role="img" tabindex="0"
    aria-label={`최근 ${days.length}일 가문 창설·통합·해산 일별 막대 그래프. 좌우 화살표로 날짜 이동.`}
    onfocus={() => { hovered ??= days.length - 1 }} onblur={() => { hovered = null }}
    onkeydown={(event) => move(hovered ?? days.length - 1, event)} onpointerleave={() => { hovered = null }}>
    {#each ticks as tick (tick)}
      <line x1={left} x2={left + plotWidth} y1={y(tick)} y2={y(tick)} class="grid-line" class:baseline={tick === 0} />
      <text x={left - 10} y={y(tick) + 4} text-anchor="end" class="axis-label">{tick}</text>
    {/each}
    {#each days as day, index (day.day_start)}
      {#if hovered === index}
        <rect x={left + band * index} y={top} width={band} height={y(0) - top} fill="#f1f5f3" />
      {/if}
      {#each dailySeries as series, slot (series.key)}
        {#if day[series.key] > 0}
          <path d={barPath(groupX(index) + slot * (bar + 2), day[series.key])} fill={series.color} />
        {/if}
      {/each}
      {#if (days.length - 1 - index) % labelEvery === 0}
        <text x={left + band * (index + 0.5)} y={height - 8} text-anchor="middle" class="axis-label">{formatShortDay(day.day_start)}</text>
      {/if}
      <rect x={left + band * index} y={top} width={band} height={height - top} fill="transparent"
        role="presentation" onpointerenter={() => { hovered = index }} />
    {/each}
  </svg>
  {#if hovered !== null}
    {@const selected = days[hovered]}
    <div class="chart-tooltip" style:left={`${Math.max(8, Math.min(width - 180, left + band * (hovered + 0.5) - 85))}px`}>
      <span>{formatDay(selected.day_start)}</span>
      {#each dailySeries as series (series.key)}
        <div class="row"><i style:background={series.color}></i>{series.label}<b>{formatCount(selected[series.key])}</b></div>
      {/each}
    </div>
  {/if}
</div>
<div class="chart-legend" aria-label="그래프 범례">
  {#each dailySeries as series (series.key)}
    <span class="legend"><i style:background={series.color}></i>{series.label}</span>
  {/each}
</div>

<style>
  .chart-canvas { margin-top: 14px; }
  .baseline { stroke: #dde5e0; stroke-dasharray: none; }
  .chart-tooltip { width: 170px; gap: 5px; }
  .row { display: flex; align-items: center; gap: 7px; color: #4f6158; font-size: 12px; }
  .row i, .legend i { width: 8px; height: 8px; border-radius: 2px; }
  .row b { margin-left: auto; color: #24332c; font-weight: 600; font-variant-numeric: tabular-nums; }
</style>
