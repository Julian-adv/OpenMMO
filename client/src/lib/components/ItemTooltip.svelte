<script lang="ts">
  import type { ItemDefinition } from '../data/itemDefs'

  interface Props {
    def: ItemDefinition
    side?: 'left' | 'right'
  }

  let { def, side = 'right' }: Props = $props()
</script>

<div class="tooltip" class:left={side === 'left'}>
  <div class="tooltip-name">{def.name}</div>
  <div class="tooltip-desc">{def.description}</div>
  <div class="tooltip-stats">
    <span>Weight: {def.weight}</span>
    {#if def.equipSlot}
      <span>Slot: {def.equipSlot.replace(/_/g, ' ')}</span>
    {/if}
  </div>
</div>

<style>
  .tooltip {
    position: absolute;
    left: calc(100% + 8px);
    top: 0;
    width: 160px;
    padding: 8px;
    background: rgba(6, 10, 14, 0.9);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
    pointer-events: none;
    z-index: 50;
  }

  .tooltip.left {
    left: auto;
    right: calc(100% + 8px);
  }

  .tooltip-name {
    font-size: 15px;
    font-weight: 700;
    color: #f0c040;
    margin-bottom: 4px;
  }

  .tooltip-desc {
    font-size: 13px;
    color: #9fb2c3;
    margin-bottom: 6px;
  }

  .tooltip-stats {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
    color: #c8d6e0;
  }
</style>
