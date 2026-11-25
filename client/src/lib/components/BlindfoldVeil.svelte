<script lang="ts">
  import { inventoryStore } from '../stores/inventoryStore'
  import { skillsStore } from '../stores/skillsStore'
  import { coversEyes } from '../data/itemDefs'
  import { cameraDistance } from '../stores/cameraStore'

  const covered = $derived(
    coversEyes($inventoryStore.equipped.head?.item_def_id)
  )
  const telepathic = $derived($skillsStore.learned.includes('telepathy'))
</script>

{#if covered}
  <div class="veil" class:telepathic style:--zoom={$cameraDistance || 1}></div>
{/if}

<style>
  /* Eyes covered: only what stands within arm's reach shows, growing with
     the camera zoom. Under the HUD. */
  .veil {
    position: fixed;
    inset: 0;
    z-index: 1;
    pointer-events: none;
    background: radial-gradient(
      circle at 50% 46%,
      transparent 0,
      transparent calc(50px * var(--zoom)),
      #000 calc(130px * var(--zoom))
    );
  }

  /* Telepathy gives the world back, dimmed and tinted. */
  .veil.telepathic {
    background: radial-gradient(
      circle at 50% 46%,
      transparent 0,
      rgba(24, 8, 48, 0.42) 75%
    );
  }
</style>
