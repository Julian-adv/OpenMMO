<script lang="ts">
  import { onMount } from 'svelte'
  import { formatDateTime, formatTime, nearestSample, splitSegments, type ConcurrentHistory } from './metrics'

  let { history }: { history: ConcurrentHistory } = $props()
  let container: HTMLDivElement
  let width = $state(1000)
  let selectedTime = $state<number | null>(null)
  let height = $derived(width < 600 ? 260 : 320)
  const left = 42
  const right = 18
  const top = 24
  const bottom = 38
  let plotWidth = $derived(Math.max(1, width - left - right))
  let plotHeight = $derived(height - top - bottom)
  let step = $derived.by(() => {
    const raw = Math.max(1, Math.max(0, ...history.samples.map((sample) => sample.accounts)) / 4)
    const magnitude = 10 ** Math.floor(Math.log10(raw))
    return ([1, 2, 5, 10].find((value) => value * magnitude >= raw) ?? 10) * magnitude
  })
  let ceiling = $derived(step * 4)
  let segments = $derived(splitSegments(history.samples, history.sample_interval_seconds))
  let selectedIndex = $derived(selectedTime === null ? null : nearestSample(history.samples, selectedTime))
  let selected = $derived(selectedIndex === null ? null : history.samples[selectedIndex])
  const x = (timestamp: number) => left + (timestamp - history.from) / (history.until - history.from) * plotWidth
  const y = (accounts: number) => top + plotHeight * (1 - accounts / ceiling)
  let tooltipLeft = $derived(selected ? Math.max(8, Math.min(width - 188, x(selected.timestamp) - 88)) : 0)

  function line(samples: ConcurrentHistory['samples']) {
    return samples.map((sample, index) => `${index === 0 ? 'M' : 'L'}${x(sample.timestamp)},${y(sample.accounts)}`).join(' ')
  }

  function selectAtPointer(event: PointerEvent) {
    const fraction = Math.max(0, Math.min(1, (event.clientX - container.getBoundingClientRect().left - left) / plotWidth))
    const timestamp = history.from + fraction * (history.until - history.from)
    const index = nearestSample(history.samples, timestamp)
    selectedTime = index !== null && Math.abs(history.samples[index].timestamp - timestamp) <= Math.max(60, (history.until - history.from) * 12 / plotWidth)
      ? history.samples[index].timestamp : null
  }

  onMount(() => {
    const observer = new ResizeObserver(([entry]) => { width = entry.contentRect.width })
    observer.observe(container)
    return () => observer.disconnect()
  })
</script>

<div class="chart-canvas" bind:this={container}>
  <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`최근 ${(history.until - history.from) / 3600}시간 동시 접속 계정 수 그래프. ${history.samples.length}개 기록.`}
    onpointermove={selectAtPointer} onpointerleave={() => { selectedTime = null }}>
    <defs>
      <linearGradient id="chart-fill" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stop-color="#168878" stop-opacity="0.16" />
        <stop offset="100%" stop-color="#168878" stop-opacity="0.015" />
      </linearGradient>
    </defs>
    {#each [4, 3, 2, 1, 0] as tick (tick)}
      <line x1={left} x2={width - right} y1={y(tick * step)} y2={y(tick * step)} class="grid-line" />
      <text x={left - 14} y={y(tick * step) + 4} text-anchor="end" class="axis-label">{tick * step}</text>
    {/each}
    {#each [0, 1, 2, 3, 4, 5, 6] as tick (tick)}
      {#if width >= 600 || tick % 2 === 0}
        <text x={left + plotWidth * tick / 6} y={height - 10} text-anchor={tick === 0 ? 'start' : tick === 6 ? 'end' : 'middle'} class="axis-label">
          {formatTime(history.from + (history.until - history.from) * tick / 6)}
        </text>
      {/if}
    {/each}
    {#each segments as segment (segment[0].timestamp)}
      {#if segment.length > 1}
        {@const path = line(segment)}
        <path d={`${path} L${x(segment[segment.length - 1].timestamp)},${y(0)} L${x(segment[0].timestamp)},${y(0)} Z`} fill="url(#chart-fill)" />
        <path d={path} fill="none" stroke="#168878" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round" />
      {:else}
        <circle cx={x(segment[0].timestamp)} cy={y(segment[0].accounts)} r="3.5" fill="#168878" />
      {/if}
    {/each}
    {#if selected}
      <line x1={x(selected.timestamp)} x2={x(selected.timestamp)} y1={top} y2={y(0)} stroke="#83b7ac" stroke-dasharray="4 4" />
      <circle cx={x(selected.timestamp)} cy={y(selected.accounts)} r="5" fill="#168878" stroke="white" stroke-width="2.5" />
    {/if}
  </svg>
  {#if selected}
    <div class="chart-tooltip" style:left={`${tooltipLeft}px`}>
      <span>{formatDateTime(selected.timestamp)}</span>
      <strong>{selected.accounts.toLocaleString('ko-KR')} <small>계정</small></strong>
    </div>
  {/if}
</div>
<div class="chart-controls">
  <span class="legend"><i></i> 접속 계정 수</span>
  <label class="chart-scrubber">
    <span>시간별 보기</span>
    <input type="range" min="0" max={Math.max(0, history.samples.length - 1)} step="1"
      value={selectedIndex ?? history.samples.length - 1} aria-label="기록 시점 선택"
      aria-valuetext={selected ? `${formatDateTime(selected.timestamp)}, ${selected.accounts}계정` : '방향키로 기록 시점을 선택하세요'}
      oninput={(event) => { selectedTime = history.samples[Number(event.currentTarget.value)].timestamp }}
      onfocus={() => { selectedTime ??= history.samples[history.samples.length - 1].timestamp }} />
  </label>
</div>
