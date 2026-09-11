<script lang="ts">
  import ConnectionBreakdown from './ConnectionBreakdown.svelte'
  import { connectionKinds, formatAxisTime, formatCount, formatDateTime, formatPeriod, nearestSample, splitSegments, type ConcurrentHistory, type Sample } from './metrics'

  let { history, peak }: { history: ConcurrentHistory; peak: number | null } = $props()
  let container: HTMLDivElement
  let width = $state(1000)
  let selectedTime = $state<number | null>(null)
  let height = $derived(width < 600 ? 260 : 320)
  let hours = $derived((history.until - history.from) / 3600)
  let legend = $derived(history.sample_interval_seconds > 60 ? '평균 접속 계정 수' : '접속 계정 수')
  let visibleKinds = $derived(connectionKinds.filter((kind) => kind.key !== 'other_accounts' || history.samples.some((sample) => sample.other_accounts > 0)))
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

  function line(samples: Sample[]) {
    return samples.map((sample, index) => `${index === 0 ? 'M' : 'L'}${x(sample.timestamp)},${y(sample.accounts)}`).join(' ')
  }

  function stackHeight(sample: Sample, layer: number) {
    if (layer === connectionKinds.length - 1) return sample.accounts
    return connectionKinds.slice(0, layer + 1).reduce((total, kind) => total + sample[kind.key], 0)
  }

  function area(samples: Sample[], layer: number) {
    const upper = samples.map((sample, index) => `${index === 0 ? 'M' : 'L'}${x(sample.timestamp)},${y(stackHeight(sample, layer))}`)
    const lower = samples.map((sample) => `L${x(sample.timestamp)},${y(stackHeight(sample, layer - 1))}`).reverse()
    return `${upper.join(' ')} ${lower.join(' ')} Z`
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
  <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`최근 ${formatPeriod(hours)} ${legend} 그래프. 웹 접속과 외부 에이전트 등의 구성을 색상별로 누적 표시합니다. ${history.samples.length}개 지점.${peak === null ? '' : ` 기간 최고 접속 ${formatCount(peak)}계정을 가로 점선으로 표시합니다.`}`}
    onpointermove={selectAtPointer} onpointerleave={() => { selectedTime = null }}>
    {#each [4, 3, 2, 1, 0] as tick (tick)}
      <line x1={left} x2={width - right} y1={y(tick * step)} y2={y(tick * step)} class="grid-line" />
      <text x={left - 14} y={y(tick * step) + 4} text-anchor="end" class="axis-label">{tick * step}</text>
    {/each}
    {#each [0, 1, 2, 3, 4, 5, 6] as tick (tick)}
      {#if width >= 600 || tick % 2 === 0}
        <text x={left + plotWidth * tick / 6} y={height - 10} text-anchor={tick === 0 ? 'start' : tick === 6 ? 'end' : 'middle'} class="axis-label">
          {formatAxisTime(history.from + (history.until - history.from) * tick / 6, hours)}
        </text>
      {/if}
    {/each}
    {#each segments as segment (segment[0].timestamp)}
      {#if segment.length > 1}
        {#each connectionKinds as kind, layer (kind.key)}
          <path d={area(segment, layer)} fill={kind.color} fill-opacity="0.45" />
        {/each}
        <path d={line(segment)} fill="none" stroke="#31594f" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round" />
      {:else}
        {#each connectionKinds as kind, layer (kind.key)}
          <line x1={x(segment[0].timestamp)} x2={x(segment[0].timestamp)} y1={y(stackHeight(segment[0], layer - 1))} y2={y(stackHeight(segment[0], layer))} stroke={kind.color} stroke-width="5" stroke-opacity="0.45" />
        {/each}
        <circle cx={x(segment[0].timestamp)} cy={y(segment[0].accounts)} r="3.5" fill="#31594f" />
      {/if}
    {/each}
    {#if peak !== null}
      <line x1={left} x2={width - right} y1={y(peak)} y2={y(peak)} class="peak-line" />
      <text x={width - right} y={y(peak) - 8} text-anchor="end" class="peak-label">기간 최고 접속 {formatCount(peak)}계정</text>
    {/if}
    {#if selected}
      <line x1={x(selected.timestamp)} x2={x(selected.timestamp)} y1={top} y2={y(0)} stroke="#83b7ac" stroke-dasharray="4 4" />
      <circle cx={x(selected.timestamp)} cy={y(selected.accounts)} r="5" fill="#31594f" stroke="white" stroke-width="2.5" />
    {/if}
  </svg>
  {#if selected}
    <div class="chart-tooltip" style:left={`${tooltipLeft}px`}>
      <span>{formatDateTime(selected.timestamp)}</span>
      <strong>{formatCount(selected.accounts)} <small>{history.sample_interval_seconds > 60 ? '계정 합계 (평균)' : '계정 합계'}</small></strong>
      <ConnectionBreakdown sample={selected} />
    </div>
  {/if}
</div>
<div class="chart-legend" aria-label="그래프 범례">
  <span class="legend"><i class="total-line"></i>합계</span>
  {#each visibleKinds as kind (kind.key)}
    <span class="legend"><i style:background={kind.color}></i>{kind.label}</span>
  {/each}
  {#if peak !== null}
    <span class="legend"><i class="peak-line"></i>기간 최고 접속</span>
  {/if}
</div>
