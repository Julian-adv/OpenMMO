<script lang="ts">
  import { inventoryStore } from '../stores/inventoryStore'
  import { getItemDef, isConsumable } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import {
    quickslots,
    QUICKSLOT_COUNT,
    loadQuickslots,
    clearQuickslot,
  } from '../stores/quickslotStore'
  import { dragMeta } from '../stores/dragStore'
  import { itemTooltip } from '../actions/itemTooltip'

  interface Props {
    /** Active character id — used to load that character's saved quickslots. */
    characterId: number | null
  }

  let { characterId }: Props = $props()

  $effect(() => {
    if (characterId != null) loadQuickslots(characterId)
  })

  /** Total quantity of an item def currently sitting in the bag. */
  function bagQuantity(defId: string): number {
    let total = 0
    for (const item of $inventoryStore.bag) {
      if (item.item_def_id === defId) total += item.quantity
    }
    return total
  }

  const slots = $derived(
    $quickslots.map((defId) => {
      if (!defId) return null
      const def = getItemDef(defId)
      if (!def) return null
      return { defId, def, qty: bagQuantity(defId) }
    })
  )

  /** Use the item bound to a quickslot: equip if equippable, else consume. */
  function useSlot(index: number) {
    const entry = slots[index]
    if (!entry) return
    const inst = $inventoryStore.bag.find((b) => b.item_def_id === entry.defId)
    if (!inst) return // none left in bag
    if (entry.def.equipSlot) {
      networkManager.sendEquipItem(inst.instance_id)
    } else if (isConsumable(entry.def)) {
      networkManager.sendUseItem(inst.instance_id)
    }
  }

  // Digit1..Digit9 → slots 0..8, Digit0 → slot 9.
  function handleKeydown(event: KeyboardEvent) {
    if (event.ctrlKey || event.altKey || event.metaKey) return
    const tag = (document.activeElement?.tagName ?? '').toLowerCase()
    if (tag === 'input' || tag === 'textarea') return
    const match = /^Digit(\d)$/.exec(event.code)
    if (!match) return
    const digit = Number(match[1])
    const index = digit === 0 ? 9 : digit - 1
    if (index >= QUICKSLOT_COUNT) return
    event.preventDefault()
    useSlot(index)
  }

  // The 1-based key label shown on each slot (last slot is "0").
  function keyLabel(index: number): string {
    return index === 9 ? '0' : String(index + 1)
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="quickslot-bar"
  class:drop-target={$dragMeta?.source.type === 'bag'}
  role="toolbar"
  aria-label="Quickslots"
>
  {#each slots as entry, i (i)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="quickslot"
      class:empty={!entry}
      data-quickslot={i}
      use:itemTooltip={entry ? { def: entry.def, side: 'right' } : null}
      ondblclick={() => useSlot(i)}
      oncontextmenu={(e) => {
        e.preventDefault()
        clearQuickslot(i)
      }}
    >
      <span class="key-label">{keyLabel(i)}</span>
      {#if entry}
        <img
          class="item-icon"
          class:depleted={entry.qty === 0}
          src="/items/{entry.def.icon}"
          alt=""
          draggable="false"
        />
        {#if entry.qty !== 1}
          <span class="item-qty" class:zero={entry.qty === 0}>{entry.qty}</span>
        {/if}
      {/if}
    </div>
  {/each}
</div>

<style>
  .quickslot-bar {
    /* Cap at 40px (~70% of the original 56px), but shrink to fit so 10 slots
       never overflow. Overhead = 9 gaps (6px) + 2 paddings (8px) + 32px margin. */
    --quickslot-size: min(40px, (100vw - 102px) / 10);
    --quickslot-gap: 4px;
    position: fixed;
    left: 50%;
    bottom: 4px;
    transform: translateX(-50%);
    z-index: 35;
    display: flex;
    flex-direction: row;
    gap: var(--quickslot-gap);
    padding: 4px;
    /* No container box — only the slots show. Border stays transparent so it
       can turn green while a bag item is dragged over the bar (.drop-target). */
    border: 1px solid transparent;
    border-radius: 10px;
    font-family: 'Courier New', monospace;
    pointer-events: auto;
    max-width: calc(100vw - 32px);
  }

  .quickslot-bar.drop-target {
    border-color: rgba(88, 255, 88, 0.5);
    box-shadow: inset 0 0 12px rgba(88, 255, 88, 0.15);
  }

  .quickslot {
    position: relative;
    box-sizing: border-box;
    width: var(--quickslot-size);
    height: var(--quickslot-size);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: rgba(6, 10, 14, 0.55);
    backdrop-filter: blur(4px);
  }

  .key-label {
    position: absolute;
    top: 2px;
    left: 4px;
    font-size: 11px;
    font-weight: 700;
    color: #9fb2c3;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.9);
    pointer-events: none;
  }

  .item-icon {
    position: absolute;
    width: var(--quickslot-size);
    height: var(--quickslot-size);
    image-rendering: pixelated;
  }

  .item-icon.depleted {
    filter: grayscale(1) brightness(0.5);
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

  .item-qty.zero {
    color: #e06c6c;
  }

  /* Very narrow (<1000px): wrap the 10 slots into two rows of five. Halves the
     bar's width so the bottom-corner panels (chat, menu buttons) have room to
     sit at the bottom instead of being lifted above the bar. */
  @media (max-width: 999.98px) {
    .quickslot-bar {
      flex-wrap: wrap;
      justify-content: center;
      --quickslot-size: 40px;
      /* Exactly five slots + four gaps per row (+1px guards against rounding
         bumping the fifth slot to a new row). */
      width: calc(5 * var(--quickslot-size) + 4 * var(--quickslot-gap) + 1px);
      max-width: calc(100vw - 18px);
    }
  }

  /* Desktop-narrow: tuck the bar against the right-hand menu buttons instead of
     centring it, freeing the whole left side for a wider chat panel. The menu
     block is two rows below 1000px (where the bar is also two rows) and one row
     above it, so it reads the matching --menu-block-* width plus a 9px screen
     margin and a 16px gap. Widths come from .game-hud (the common ancestor). */
  @media (min-width: 601px) and (max-width: 999.98px) and (pointer: fine) {
    .quickslot-bar {
      left: auto;
      right: calc(var(--menu-block-2row, 124px) + 9px + 16px);
      transform: none;
    }
  }

  @media (min-width: 1000px) and (max-width: 1200px) {
    .quickslot-bar {
      left: auto;
      right: calc(var(--menu-block-1row, 212px) + 9px + 16px);
      transform: none;
    }
  }

  @media (max-width: 600px), (pointer: coarse) {
    .quickslot-bar {
      bottom: calc(2px + env(safe-area-inset-bottom));
      padding: 6px;
    }
  }
</style>
