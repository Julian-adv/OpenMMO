<script lang="ts">
  import ConnectionBreakdown from './ConnectionBreakdown.svelte'
  import HistoryChart from './HistoryChart.svelte'
  import { connectionKinds, type ChartMarker, type ConcurrentHistory, type Sample } from './metrics'

  let { history, peak, markers = [] }: { history: ConcurrentHistory; peak: number | null; markers?: ChartMarker[] } = $props()
  let visibleKinds = $derived(connectionKinds.filter((kind) => kind.key !== 'other_accounts' || history.samples.some((sample) => sample.other_accounts > 0)))

  function stackHeight(sample: Sample, layer: number) {
    if (layer === connectionKinds.length - 1) return sample.accounts
    return connectionKinds.slice(0, layer + 1).reduce((total, kind) => total + sample[kind.key], 0)
  }

  function area(samples: Sample[], layer: number, x: (timestamp: number) => number, y: (accounts: number) => number) {
    const upper = samples.map((sample, index) => `${index === 0 ? 'M' : 'L'}${x(sample.timestamp)},${y(stackHeight(sample, layer))}`)
    const lower = samples.map((sample) => `L${x(sample.timestamp)},${y(stackHeight(sample, layer - 1))}`).reverse()
    return `${upper.join(' ')} ${lower.join(' ')} Z`
  }
</script>

<HistoryChart {history} {peak} {markers} value={(sample) => sample.accounts} legend="평균 접속 계정 수" legendLabel="합계" valueLabel="계정 합계 (평균)">
  {#snippet layers(segment, x, y)}
    {#each connectionKinds as kind, layer (kind.key)}
      {#if segment.length > 1}
        <path d={area(segment, layer, x, y)} fill={kind.color} fill-opacity="0.45" />
      {:else}
        <line x1={x(segment[0].timestamp)} x2={x(segment[0].timestamp)} y1={y(stackHeight(segment[0], layer - 1))} y2={y(stackHeight(segment[0], layer))} stroke={kind.color} stroke-width="5" stroke-opacity="0.45" />
      {/if}
    {/each}
  {/snippet}
  {#snippet detail(selected)}
    <ConnectionBreakdown sample={selected} />
  {/snippet}
  {#snippet legends()}
    {#each visibleKinds as kind (kind.key)}
      <span class="legend"><i style:background={kind.color}></i>{kind.label}</span>
    {/each}
  {/snippet}
</HistoryChart>
