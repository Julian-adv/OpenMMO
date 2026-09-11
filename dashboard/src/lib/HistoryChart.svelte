<script lang="ts" generics="T extends AccountSample">
  import type { Snippet } from 'svelte'
  import { formatAxisTime, formatCount, formatDateTime, formatPeriod, nearestSample, splitSegments, type AccountSample, type ChartHistory } from './metrics'

  let { history, peak, legend, legendLabel = legend, valueLabel, peakLabel = '기간 최고 접속', layers, detail, legends }: {
    history: ChartHistory<T>
    peak: number | null
    legend: string
    legendLabel?: string
    valueLabel: string
    peakLabel?: string
    layers?: Snippet<[T[], (timestamp: number) => number, (accounts: number) => number]>
    detail?: Snippet<[T]>
    legends?: Snippet
  } = $props()
  let container: HTMLDivElement
  let width = $state(1000)
  let selectedTime = $state<number | null>(null)
  let height = $derived(width < 600 ? 260 : 320)
  let hours = $derived((history.until - history.from) / 3600)
  let daily = $derived(history.sample_interval_seconds >= 86400)
  let axisTicks = $derived(daily && hours <= 24 ? [0, 6] : width >= 600 ? [0, 1, 2, 3, 4, 5, 6] : hours > 24 && hours <= 4320 ? [0, 3, 6] : [0, 2, 4, 6])
  const left = 42
  const right = 18
  const top = 24
  const bottom = 38
  let plotWidth = $derived(Math.max(1, width - left - right))
  let plotHeight = $derived(height - top - bottom)
  let step = $derived.by(() => {
    const raw = Math.max(1, (peak ?? 0) / 4)
    const magnitude = 10 ** Math.floor(Math.log10(raw))
    return ([1, 2, 5, 10].find((value) => value * magnitude >= raw) ?? 10) * magnitude
  })
  let ceiling = $derived(step * 4)
  let segments = $derived(splitSegments(history.samples, history.sample_interval_seconds))
  let selectedIndex = $derived(selectedTime === null ? null : nearestSample(history.samples, selectedTime))
  let selected = $derived(selectedIndex === null ? null : history.samples[selectedIndex])
  const x = (timestamp: number) => left + (timestamp - history.from) / (history.until - history.from) * plotWidth
  const y = (accounts: number) => top + plotHeight * (1 - accounts / ceiling)
  let tooltipLeft = $derived(selected ? Math.max(8, Math.min(width - 244, x(selected.timestamp) - 118)) : 0)

  function line(samples: T[]) {
    return samples.map((sample, index) => `${index === 0 ? 'M' : 'L'}${x(sample.timestamp)},${y(sample.accounts)}`).join(' ')
  }

  function selectAtPointer(event: PointerEvent) {
    const fraction = Math.max(0, Math.min(1, (event.clientX - container.getBoundingClientRect().left - left) / plotWidth))
    const timestamp = history.from + fraction * (history.until - history.from)
    const index = nearestSample(history.samples, timestamp)
    selectedTime = index !== null && Math.abs(history.samples[index].timestamp - timestamp) <= Math.max(history.sample_interval_seconds, (history.until - history.from) * 12 / plotWidth)
      ? history.samples[index].timestamp : null
  }
</script>

<div class="chart-canvas" bind:this={container} bind:contentRect={null, (rect: DOMRectReadOnly | null | undefined) => { if (rect) width = rect.width }}>
  <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`최근 ${formatPeriod(hours)} ${legend} 그래프. ${history.samples.length}개 지점.${peak === null ? '' : ` ${peakLabel} ${formatCount(peak)}계정을 가로 점선으로 표시합니다.`}`}
    onpointermove={selectAtPointer} onpointerleave={() => { selectedTime = null }}>
    {#each [4, 3, 2, 1, 0] as tick (tick)}
      <line x1={left} x2={width - right} y1={y(tick * step)} y2={y(tick * step)} class="grid-line" />
      <text x={left - 14} y={y(tick * step) + 4} text-anchor="end" class="axis-label">{tick * step}</text>
    {/each}
    {#each axisTicks as tick (tick)}
      <text x={left + plotWidth * tick / 6} y={height - 10} text-anchor={tick === 0 ? 'start' : tick === 6 ? 'end' : 'middle'} class="axis-label">
        {formatAxisTime(history.from + (history.until - history.from) * tick / 6, hours, daily)}
      </text>
    {/each}
    {#each segments as segment (segment[0].timestamp)}
      {#if layers}
        {@render layers(segment, x, y)}
      {:else if segment.length > 1}
        <path d={`${line(segment)} L${x(segment[segment.length - 1].timestamp)},${y(0)} L${x(segment[0].timestamp)},${y(0)} Z`} fill="#168878" fill-opacity="0.25" />
      {/if}
      {#if segment.length > 1}
        <path d={line(segment)} fill="none" stroke="#31594f" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round" />
      {:else}
        <circle cx={x(segment[0].timestamp)} cy={y(segment[0].accounts)} r="3.5" fill="#31594f" />
      {/if}
    {/each}
    {#if peak !== null}
      <line x1={left} x2={width - right} y1={y(peak)} y2={y(peak)} class="peak-line" />
      <text x={width - right} y={y(peak) - 8} text-anchor="end" class="peak-label">{peakLabel} {formatCount(peak)}계정</text>
    {/if}
    {#if selected}
      <line x1={x(selected.timestamp)} x2={x(selected.timestamp)} y1={top} y2={y(0)} stroke="#83b7ac" stroke-dasharray="4 4" />
      <circle cx={x(selected.timestamp)} cy={y(selected.accounts)} r="5" fill="#31594f" stroke="white" stroke-width="2.5" />
    {/if}
  </svg>
  {#if selected}
    <div class="chart-tooltip" style:left={`${tooltipLeft}px`}>
      <span>{formatDateTime(selected.timestamp)}</span>
      <strong>{formatCount(selected.accounts)} <small>{valueLabel}</small></strong>
      {@render detail?.(selected)}
    </div>
  {/if}
</div>
<div class="chart-legend" aria-label="그래프 범례">
  <span class="legend"><i class="total-line"></i>{legendLabel}</span>
  {@render legends?.()}
  {#if peak !== null}
    <span class="legend"><i class="peak-line"></i>{peakLabel}</span>
  {/if}
</div>
