<script lang="ts">
  import { capeTextureUrl } from '../utils/networkUtils'
  import type { Emblem } from '../utils/familyEmblem'

  let { emblem, size = 22 }: { emblem?: Emblem | null; size?: number } =
    $props()
</script>

{#if emblem?.color}
  <span class="chip" style="--size:{size}px; background:{emblem.color}"></span>
{/if}
{#if emblem?.texture}
  <img
    class="chip"
    style="--size:{size}px"
    src={capeTextureUrl(emblem.texture)}
    alt=""
  />
{/if}

<style>
  .chip {
    width: var(--size);
    height: var(--size);
    border-radius: 4px;
    object-fit: cover;
  }
</style>
