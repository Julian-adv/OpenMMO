<script lang="ts">
  import HistoryChart from './HistoryChart.svelte'
  import type { HardwareSnapshot, HardwareStatus } from './hardware'
  import { formatPercent } from './hardware'
  import { formatBytes } from './traffic'
  import { formatCount, type ChartMarker } from './metrics'

  let { history, markers = [] }: { history: HardwareStatus; markers?: ChartMarker[] } = $props()
  let metric = $state('process-memory')
  let mount = $state('/')
  const options = [
    { key: 'process-memory', label: '프로세스 메모리' },
    { key: 'process-cpu', label: '프로세스 CPU' },
    { key: 'server-memory', label: '서버 메모리' },
    { key: 'server-cpu', label: '서버 CPU' },
    { key: 'load', label: '평균 부하' },
    { key: 'disk', label: '디스크 여유 공간' },
  ]
  let mounts = $derived([...new Set(history.samples.flatMap((sample) => sample.disks.map((disk) => disk.mount)))].sort())
  let selectedMount = $derived(mounts.includes(mount) ? mount : mounts[0])
  $effect(() => {
    if (mounts.length && !mounts.includes(mount)) mount = mounts[0]
  })
  let series = $derived.by(() => {
    const client = { label: 'agent-client', color: '#31594f' }
    const llm = { label: 'LLM CLI (Codex 등)', color: '#7870cf' }
    switch (metric) {
      case 'process-cpu': return [
        { ...client, value: (sample: HardwareSnapshot) => sample.agent.instances ? sample.agent.client.cpu_percent : null },
        { ...llm, value: (sample: HardwareSnapshot) => sample.agent.instances ? sample.agent.llm.cpu_percent : null },
      ]
      case 'server-memory': return [{ label: '서버 메모리 사용량', color: '#31594f', value: (sample: HardwareSnapshot) => sample.memory_used_bytes }]
      case 'server-cpu': return [{ label: '서버 CPU', color: '#31594f', value: (sample: HardwareSnapshot) => sample.cpu_percent }]
      case 'load': return ['1분', '5분', '15분'].map((label, index) => ({
        label: `평균 부하 ${label}`, color: ['#31594f', '#7870cf', '#bf7b38'][index],
        value: (sample: HardwareSnapshot) => sample.load_average?.[index] ?? null,
      }))
      case 'disk': return [{ label: `${selectedMount ?? ''} 여유 공간`, color: '#31594f', value: (sample: HardwareSnapshot) => sample.disks.find((disk) => disk.mount === selectedMount)?.available_bytes ?? null }]
      default: return [
        { ...client, value: (sample: HardwareSnapshot) => sample.agent.instances ? sample.agent.client.memory_bytes : null },
        { ...llm, value: (sample: HardwareSnapshot) => sample.agent.instances ? sample.agent.llm.memory_bytes : null },
      ]
    }
  })
  let chart = $derived({ ...history, samples: history.samples.filter((sample) => series.every((line) => line.value(sample) !== null)) })
  let peak = $derived(chart.samples.length ? Math.max(...chart.samples.map((sample) => Math.max(...series.map((line) => line.value(sample)!)))) : null)
  let formatValue = $derived(metric.endsWith('memory') || metric === 'disk' ? formatBytes : metric.endsWith('cpu') ? formatPercent : formatCount)
  const value = (sample: HardwareSnapshot) => series[0].value(sample)!

  function path(segment: HardwareSnapshot[], getValue: (sample: HardwareSnapshot) => number | null, x: (timestamp: number) => number, y: (value: number) => number) {
    return segment.map((sample, index) => `${index === 0 ? 'M' : 'L'}${x(sample.timestamp)},${y(getValue(sample)!)}`).join(' ')
  }
</script>

<div class="chart-meta">
  <label>시간별 추이 <select bind:value={metric} aria-label="하드웨어 그래프 지표">
    {#each options as option (option.key)}<option value={option.key}>{option.label}</option>{/each}
  </select></label>
  {#if metric === 'disk'}
    <label>디스크 <select bind:value={mount} aria-label="디스크 그래프 마운트">
      {#each mounts as disk (disk)}<option value={disk}>{disk}</option>{/each}
    </select></label>
  {/if}
  <span>1분 간격 · 최근 7일 보관</span>
</div>
{#if chart.samples.length}
  {#key `${metric}:${selectedMount}`}
    <HistoryChart history={chart} {peak} {value} {markers} legend={options.find((option) => option.key === metric)!.label} legendLabel={series[0].label} valueLabel={series[0].label} unit="" peakLabel="기간 최고" {formatValue} axisWidth={95}>
      {#snippet amount(amount: number)}{formatValue(amount)}{/snippet}
      {#snippet layers(segment, x, y)}
        {#each series.slice(1) as line (line.label)}
          {#if segment.length > 1}
            <path d={path(segment, line.value, x, y)} fill="none" stroke={line.color} stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round" />
          {:else}
            <circle cx={x(segment[0].timestamp)} cy={y(line.value(segment[0])!)} r="3.5" fill={line.color} />
          {/if}
        {/each}
      {/snippet}
      {#snippet detail(sample)}
        {#each series.slice(1) as line (line.label)}<span>{line.label} {formatValue(line.value(sample)!)}</span>{/each}
      {/snippet}
      {#snippet legends()}
        {#each series.slice(1) as line (line.label)}<span class="legend"><i style:background={line.color}></i>{line.label}</span>{/each}
      {/snippet}
    </HistoryChart>
  {/key}
{:else}
  <div class="chart-empty" role="status"><strong>이 기간에 수집된 기록이 없습니다</strong><span>기록이 쌓이면 추이를 확인할 수 있습니다.</span></div>
{/if}

<style>
  select { margin-left: 8px; background: white; border: 1px solid #dde5e0; border-radius: 6px; padding: 4px; color: inherit; }
  .chart-empty { min-height: 150px; }
</style>
