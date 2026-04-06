<script lang="ts">
  import { dragMeta, dragPos } from '../stores/dragStore'
</script>

{#if $dragMeta}
  <img
    class="drag-ghost"
    src="/items/{$dragMeta.icon}"
    alt=""
    style="left:{$dragPos.x}px;top:{$dragPos.y}px"
  />
{/if}

<style>
  .drag-ghost {
    position: fixed;
    width: 48px;
    height: 48px;
    transform: translate(-50%, -50%);
    image-rendering: pixelated;
    pointer-events: none;
    z-index: 100;
    opacity: 0.85;
    filter: drop-shadow(0 2px 6px rgba(0, 0, 0, 0.6));
  }
</style>
