<script lang="ts">
  import { inventoryStore } from '../stores/inventoryStore'
  import type { ItemInstance } from '../stores/inventoryStore'
  import { getItemDef } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import type { CharacterAttributes } from '../network/networkTypes'

  interface Props {
    visible: boolean
    attributes: CharacterAttributes | null
  }

  let { visible, attributes }: Props = $props()

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

  let dragging = $state<{ icon: string; x: number; y: number } | null>(null)

  function onPointerDown(e: PointerEvent, slot: ItemInstance) {
    if (e.button !== 0) return
    e.preventDefault()
    const target = e.currentTarget as HTMLElement
    target.setPointerCapture(e.pointerId)
    const def = getItemDef(slot.item_def_id)
    const icon = def?.icon ?? 'icon_frame.png'
    const startX = e.clientX
    const startY = e.clientY
    let started = false

    function onMove(me: PointerEvent) {
      me.preventDefault()
      const dx = me.clientX - startX
      const dy = me.clientY - startY
      if (!started && dx * dx + dy * dy < 64) return
      started = true
      hoveredSlot = null
      dragging = { icon, x: me.clientX, y: me.clientY }
    }

    function onUp(ue: PointerEvent) {
      if (target.hasPointerCapture(ue.pointerId)) {
        target.releasePointerCapture(ue.pointerId)
      }
      target.removeEventListener('pointermove', onMove)
      target.removeEventListener('pointerup', onUp)
      if (!started || !panelEl) {
        dragging = null
        return
      }
      const rect = panelEl.getBoundingClientRect()
      const outside =
        ue.clientX < rect.left ||
        ue.clientX > rect.right ||
        ue.clientY < rect.top ||
        ue.clientY > rect.bottom
      if (outside) {
        networkManager.sendDropItem(slot.instance_id)
      }
      dragging = null
    }

    target.addEventListener('pointermove', onMove)
    target.addEventListener('pointerup', onUp)
  }
</script>

{#if visible}
  <div class="inventory-panel" role="dialog" aria-label="Inventory" bind:this={panelEl}>
    <div class="panel-header">
      <span class="panel-title">Inventory</span>
      <span class="weight-display">
        {(currentWeight / 10).toFixed(1)} / {(maxWeight / 10).toFixed(1)} kg
      </span>
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
          <img class="icon-frame" src="/items/icon_frame.png" alt="" draggable="false" />
          {#if def}
            <img class="item-icon" src="/items/{def.icon}" alt="" draggable="false" />
          {/if}
          {#if slot && slot.quantity > 1}
            <span class="item-qty">{slot.quantity}</span>
          {/if}
          {#if slot && def && hoveredSlot === i}
            <div class="tooltip">
              <div class="tooltip-name">{def.name}</div>
              <div class="tooltip-desc">{def.description}</div>
              <div class="tooltip-stats">
                <span>Weight: {def.weight}</span>
                {#if def.equipSlot}
                  <span>Slot: {def.equipSlot.replace('_', ' ')}</span>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

{#if dragging}
  <img
    class="drag-ghost"
    src="/items/{dragging.icon}"
    alt=""
    style="left:{dragging.x}px;top:{dragging.y}px"
  />
{/if}

<style>
  .inventory-panel {
    position: fixed;
    right: 16px;
    top: 50%;
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

  .weight-display {
    font-size: 11px;
    color: #9fb2c3;
  }

  .bag-grid {
    display: grid;
    grid-template-columns: repeat(5, 64px);
    grid-template-rows: repeat(10, 64px);
    gap: 2px;
  }

  .grid-cell {
    position: relative;
    width: 64px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .icon-frame {
    width: 64px;
    height: 64px;
    image-rendering: pixelated;
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

  .tooltip {
    position: absolute;
    left: -170px;
    top: 0;
    width: 160px;
    padding: 8px;
    background: rgba(6, 10, 14, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
    pointer-events: none;
    z-index: 50;
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
