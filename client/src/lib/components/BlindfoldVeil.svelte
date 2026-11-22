<script lang="ts">
  import { inventoryStore } from '../stores/inventoryStore'
  import { skillsStore } from '../stores/skillsStore'
  import { coversEyes } from '../data/itemDefs'

  const covered = $derived(
    coversEyes($inventoryStore.equipped.head?.item_def_id)
  )
  const telepathic = $derived($skillsStore.learned.includes('telepathy'))
</script>

{#if covered}
  <div class="veil" class:telepathic></div>
{/if}

<style>
  /* Eyes covered: only what stands within arm's reach shows. */
  .veil {
    position: fixed;
    inset: 0;
    pointer-events: none;
    background: radial-gradient(
      circle at 50% 46%,
      transparent 0,
      transparent 70px,
      rgba(0, 0, 0, 0.97) 170px
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
