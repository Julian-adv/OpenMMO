<script lang="ts">
  import { inventoryStore } from '../stores/inventoryStore'
  import type { ItemInstance } from '../stores/inventoryStore'
  import { getItemDef } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import type { CharacterAttributes, EquipSlot } from '../network/networkTypes'
  import { dragMeta, startDrag, isSlotCompatible, pointInRect, isOverAnyDialog, FALLBACK_ICON } from '../stores/dragStore'
  import ItemTooltip from './ItemTooltip.svelte'

  interface Props {
    visible: boolean
    attributes: CharacterAttributes | null
    onClose: () => void
  }

  let { visible, attributes, onClose }: Props = $props()

  const maxWeight = $derived(attributes ? attributes.str * 15 : 150)

  function itemWeight(item: ItemInstance): number {
    const def = getItemDef(item.item_def_id)
    return (def?.weight ?? 1) * item.quantity
  }

  const currentWeight = $derived.by(() => {
    const inv = $inventoryStore
    let total = 0
    for (const item of inv.bag) total += itemWeight(item)
    for (const item of Object.values(inv.equipped)) {
      if (item) total += itemWeight(item)
    }
    return total
  })

  const COLS = 5
  const ROWS = 10
  const TOTAL_SLOTS = COLS * ROWS

  const slots = $derived.by(() => {
    const bag = $inventoryStore.bag
    const result: (ItemInstance | null)[] = new Array(TOTAL_SLOTS).fill(null)
    for (let i = 0; i < bag.length && i < TOTAL_SLOTS; i++) {
      result[i] = bag[i]
    }
    return result
  })

  let hoveredSlot = $state<number | null>(null)
  let panelEl = $state<HTMLDivElement | null>(null)

  function onDblClick(slot: ItemInstance | null) {
    if (!slot) return
    const def = getItemDef(slot.item_def_id)
    if (def?.equipSlot) {
      networkManager.sendEquipItem(slot.instance_id)
    }
  }

  function onPointerDown(e: PointerEvent, slot: ItemInstance) {
    if (e.button !== 0) return
    e.preventDefault()
    hoveredSlot = null
    const def = getItemDef(slot.item_def_id)

    startDrag(
      e,
      {
        instanceId: slot.instance_id,
        equipSlot: def?.equipSlot ?? null,
        source: { type: 'bag' },
        icon: def?.icon ?? FALLBACK_ICON,
      },
      (x, y) => {
        for (const slotEl of document.querySelectorAll<HTMLElement>('[data-equip-slot]')) {
          if (pointInRect(x, y, slotEl.getBoundingClientRect())) {
            const targetSlot = slotEl.dataset.equipSlot as EquipSlot
            if (isSlotCompatible(def?.equipSlot ?? null, targetSlot)) {
              networkManager.sendEquipItem(slot.instance_id)
              return
            }
          }
        }
        if (panelEl && !pointInRect(x, y, panelEl.getBoundingClientRect()) && !isOverAnyDialog(x, y)) {
          networkManager.sendDropItem(slot.instance_id)
        }
      },
    )
  }
</script>

{#if visible}
  <div
    class="inventory-panel"
    class:drop-target={$dragMeta?.source.type === 'equipped'}
    role="dialog"
    aria-label="Inventory"
    data-panel="inventory"
    bind:this={panelEl}
  >
    <div class="panel-header">
      <span class="panel-title">Inventory</span>
      <span class="weight-display">
        {(currentWeight / 10).toFixed(1)} / {(maxWeight / 10).toFixed(1)} kg
      </span>
      <button class="close-btn" onclick={onClose}>&times;</button>
    </div>

    <div class="bag-grid">
      {#each slots as slot, i (slot?.instance_id ?? `empty-${i}`)}
        {@const def = slot ? getItemDef(slot.item_def_id) : null}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="grid-cell"
          onmouseenter={() => { if (slot) hoveredSlot = i }}
          onmouseleave={() => hoveredSlot = null}
          ondblclick={() => onDblClick(slot)}
          onpointerdown={(e: PointerEvent) => { if (slot) onPointerDown(e, slot) }}
        >
          {#if def}
            <img class="item-icon" src="/items/{def.icon}" alt="" draggable="false" />
          {/if}
          {#if slot && slot.quantity > 1}
            <span class="item-qty">{slot.quantity}</span>
          {/if}
          {#if slot && def && hoveredSlot === i}
            <ItemTooltip {def} side="left" />
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .inventory-panel {
    position: fixed;
    right: 16px;
    top: 45%;
    transform: translateY(-50%);
    z-index: 40;
    display: flex;
    flex-direction: column;
    backdrop-filter: blur(4px);
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    pointer-events: auto;
  }

  .inventory-panel.drop-target {
    border-color: rgba(88, 255, 88, 0.5);
    box-shadow: inset 0 0 12px rgba(88, 255, 88, 0.15);
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 8px;
  }

  .panel-title {
    font-size: 14px;
    font-weight: 700;
    color: #f0c040;
  }

  .close-btn {
    background: none;
    border: none;
    color: #9fb2c3;
    font-size: 18px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
  }

  .close-btn:hover {
    color: #fff;
  }

  .weight-display {
    font-size: 11px;
    color: #9fb2c3;
  }

  .bag-grid {
    display: grid;
    grid-template-columns: repeat(5, 64px);
    grid-template-rows: repeat(10, 64px);
    gap: 6px;
  }

  .grid-cell {
    position: relative;
    width: 64px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
  }

  .item-icon {
    position: absolute;
    width: 64px;
    height: 64px;
    image-rendering: pixelated;
  }

  .item-qty {
    position: absolute;
    bottom: 2px;
    right: 4px;
    font-size: 11px;
    font-weight: 700;
    color: #fff;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.8);
  }

</style>
