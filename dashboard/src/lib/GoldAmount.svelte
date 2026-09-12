<script lang="ts">
  import { goldSegments } from './metrics'

  let { copper, svg = false }: { copper: number, svg?: boolean } = $props()
  let segments = $derived(goldSegments(copper))
</script>

{#if svg}
  {#each segments as segment, index (segment.unit)}{#if index > 0}{' '}{/if}<tspan class={segment.unit}>{segment.text}</tspan>{/each}
{:else}
  <span class="gold-amount">{#each segments as segment, index (segment.unit)}{#if index > 0}{' '}{/if}<span class={segment.unit}>{segment.text}</span>{/each}</span>
{/if}

<style>
  .gold-amount { white-space: nowrap; font-variant-numeric: tabular-nums; }
  .gold { color: #d4b000; }
  .silver { color: #8a95a3; }
  .copper { color: #b87333; }
  tspan { fill: currentColor; }
</style>
