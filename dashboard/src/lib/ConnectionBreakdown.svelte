<script lang="ts">
  import { formatCount, visibleConnectionParts, type Sample } from './metrics'

  let { sample }: { sample: Sample } = $props()
  let parts = $derived(visibleConnectionParts(sample))
</script>

<div class="connection-breakdown">
  <div class="composition-bar" aria-hidden="true">
    {#each parts as part (part.key)}
      <span style:width={`${part.percent}%`} style:background={part.color}></span>
    {/each}
  </div>
  <ul class="composition-list" aria-label="접속 구성">
    {#each parts as part (part.key)}
      <li>
        <span class="composition-label"><i style:background={part.color} aria-hidden="true"></i>{part.label}</span>
        <span class="composition-count">{formatCount(part.accounts)}계정</span>
        <span class="composition-percent">{formatCount(part.percent)}%</span>
      </li>
    {/each}
  </ul>
</div>
