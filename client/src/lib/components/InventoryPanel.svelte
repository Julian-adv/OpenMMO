<script lang="ts">
  import { t, locale } from '../i18n'
  import { itemDisplayName } from '../data/itemDefs'
  import {
    inventoryStore,
    itemLockMode,
    playerGold,
    carryWeight,
    maxCarryWeight,
    wornAmmoStack,
  } from '../stores/inventoryStore'
  import { hungerState } from '../stores/hungerStore'
  import type { ItemInstance } from '../stores/inventoryStore'
  import { getItemDef, isUsable, type ItemDefinition } from '../data/itemDefs'
  import GoldAmount from './GoldAmount.svelte'
  import { networkManager } from '../network/socket'
  import type { EquipSlot } from '../network/networkTypes'
  import {
    dragMeta,
    startDrag,
    isSlotCompatible,
    pointInRect,
    isOverAnyDialog,
    FALLBACK_ICON,
  } from '../stores/dragStore'
  import { itemTooltip } from '../actions/itemTooltip'
  import { buildInventorySlots } from './inventorySlots'
  import { inventoryGroupKey, sortBag } from './inventorySort'
  import ItemLockButton from './ItemLockButton.svelte'
  import { groupBagForSelection, splitGroupQty } from './inventoryGroups'
  import QuantityPopup from './QuantityPopup.svelte'
  import { playerTrade, reservedQuantity } from '../stores/playerTradeStore'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { draggablePanel } from '../actions/draggablePanel'
  import WeightBar from './WeightBar.svelte'

  interface Props {
    visible: boolean
    str: number
    onClose: () => void
  }

  let { visible, str, onClose }: Props = $props()

  const maxWeight = $derived(maxCarryWeight(str, $hungerState))

  // The character panel already displays the worn quiver.
  const wornId = $derived(wornAmmoStack($inventoryStore)?.instance_id)
  const slots = $derived(
    buildInventorySlots(
      sortBag($inventoryStore.bag.filter((item) => item.instance_id !== wornId))
    )
  )

  let panelEl = $state<HTMLDivElement | null>(null)
  let pendingDrop = $state<{ slot: ItemInstance; def: ItemDefinition } | null>(
    null
  )

  // Select mode groups non-stackable items for bulk dropping.
  let selectMode = $state(false)
  const selected = new SvelteSet<number>()
  const bagIds = $derived(
    new Set(
      $inventoryStore.bag
        .filter((item) => !item.locked)
        .map((item) => item.instance_id)
    )
  )

  // Closing the panel resets its editing modes.
  $effect(() => {
    if (!visible) {
      selectMode = false
      itemLockMode.set(false)
      selected.clear()
      return
    }
    for (const id of selected) {
      if (!bagIds.has(id)) selected.delete(id)
    }
  })

  function liveBagIds(ids: Iterable<number>): number[] {
    return [...ids].filter((id) => bagIds.has(id))
  }

  function isSelectable(slot: ItemInstance): boolean {
    return !slot.locked && getItemDef(slot.item_def_id)?.stackable !== true
  }

  function lockTargets(slot: ItemInstance): number[] {
    const key = inventoryGroupKey(slot)
    return $inventoryStore.bag
      .filter(
        (item) => item.instance_id !== wornId && inventoryGroupKey(item) === key
      )
      .map((item) => item.instance_id)
  }

  function toggleSelectMode() {
    selectMode = !selectMode
    itemLockMode.set(false)
    selected.clear()
  }

  function toggleLockMode() {
    itemLockMode.update((active) => !active)
    selectMode = false
    selected.clear()
  }

  function toggleSelected(slot: ItemInstance) {
    if (!isSelectable(slot)) return
    if (selected.has(slot.instance_id)) selected.delete(slot.instance_id)
    else selected.add(slot.instance_id)
  }

  /** Split merged display stacks across their real bag instances. */
  function dropQty(slot: ItemInstance, qty: number) {
    if (slot.locked) return
    const key = inventoryGroupKey(slot)
    const group = groupBagForSelection(
      $inventoryStore.bag.filter((item) => !item.locked)
    ).find((g) => g.key === key)
    if (!group) return
    const lines = splitGroupQty(group, Math.min(qty, group.totalQty))
    networkManager.sendDropItems(
      lines.map((l) => ({ instance_id: l.instanceId, qty: l.qty }))
    )
  }

  function confirmPendingDrop(qty: number) {
    if (!pendingDrop) return
    dropQty(pendingDrop.slot, qty)
    pendingDrop = null
  }

  function cancelPendingDrop() {
    pendingDrop = null
  }

  function onDblClick(slot: ItemInstance | null) {
    if (!slot || selectMode) return
    const def = getItemDef(slot.item_def_id)
    if (def?.equipSlot) {
      networkManager.sendEquipItem(slot.instance_id)
    } else if (def && isUsable(def)) {
      networkManager.sendUseItem(slot.instance_id)
    } else if (def?.ammoKind) {
      // Ammunition has no `equipSlot` — a stackable cannot hold one — so it
      // gets its own branch rather than a slot. Same gesture either way.
      networkManager.sendSelectAmmo(slot.item_def_id)
    }
  }

  /** Use startDrag's click callback to avoid reselecting a dropped item. */
  function onGroupPointerDown(e: PointerEvent, slot: ItemInstance) {
    const def = getItemDef(slot.item_def_id)
    const groupIds = new SvelteSet(liveBagIds(selected))
    groupIds.add(slot.instance_id)
    // Merge matching items into one drag icon.
    const byDefId = new SvelteMap<string, { icon: string; quantity: number }>()
    for (const id of groupIds) {
      const item = $inventoryStore.bag.find((i) => i.instance_id === id)
      if (!item) continue
      const existing = byDefId.get(item.item_def_id)
      if (existing) {
        existing.quantity += item.quantity
      } else {
        byDefId.set(item.item_def_id, {
          icon: getItemDef(item.item_def_id)?.icon ?? FALLBACK_ICON,
          quantity: item.quantity,
        })
      }
    }
    const groupItems = [...byDefId.values()]

    startDrag(
      e,
      {
        instanceId: slot.instance_id,
        defId: slot.item_def_id,
        enchant: slot.enchant,
        // Group drags never equip, regardless of drop position.
        equipSlot: null,
        source: { type: 'bag' },
        icon: def?.icon ?? FALLBACK_ICON,
        groupItems,
      },
      (x, y) => {
        if (
          panelEl &&
          !slot.locked &&
          !pointInRect(x, y, panelEl.getBoundingClientRect()) &&
          !isOverAnyDialog(x, y)
        ) {
          const dropIds = liveBagIds(groupIds)
          if (dropIds.length > 0) {
            networkManager.sendDropItems(
              dropIds.map((instance_id) => ({ instance_id, qty: 1 }))
            )
          }
          selected.clear()
        }
      },
      () => toggleSelected(slot)
    )
  }

  function onPointerDown(e: PointerEvent, slot: ItemInstance) {
    if (e.button !== 0) return
    if (selectMode) {
      if (!isSelectable(slot)) return
      e.preventDefault()
      onGroupPointerDown(e, slot)
      return
    }
    e.preventDefault()
    const def = getItemDef(slot.item_def_id)

    startDrag(
      e,
      {
        instanceId: slot.instance_id,
        defId: slot.item_def_id,
        enchant: slot.enchant,
        equipSlot: def?.equipSlot ?? null,
        source: { type: 'bag' },
        icon: def?.icon ?? FALLBACK_ICON,
      },
      (x, y) => {
        for (const slotEl of document.querySelectorAll<HTMLElement>(
          '[data-equip-slot]'
        )) {
          if (pointInRect(x, y, slotEl.getBoundingClientRect())) {
            const targetSlot = slotEl.dataset.equipSlot as EquipSlot
            if (isSlotCompatible(def?.equipSlot ?? null, targetSlot)) {
              networkManager.sendEquipItem(slot.instance_id)
              return
            }
          }
        }
        // The quiver cell is not an equip slot — a stackable cannot hold one —
        // so it advertises the kind it takes instead of a slot name.
        for (const cell of document.querySelectorAll<HTMLElement>(
          '[data-ammo-kind]'
        )) {
          if (
            pointInRect(x, y, cell.getBoundingClientRect()) &&
            def?.ammoKind &&
            def.ammoKind === cell.dataset.ammoKind
          ) {
            networkManager.sendSelectAmmo(slot.item_def_id)
            return
          }
        }
        if (
          panelEl &&
          !pointInRect(x, y, panelEl.getBoundingClientRect()) &&
          !isOverAnyDialog(x, y)
        ) {
          if (slot.locked) return
          if (slot.quantity > 1 && def) {
            pendingDrop = { slot, def }
          } else {
            networkManager.sendDropItem(slot.instance_id)
          }
        }
      }
    )
  }
</script>

{#if visible}
  <div
    class="inventory-panel"
    class:drop-target={$dragMeta?.source.type === 'equipped'}
    role="dialog"
    aria-label={$t('inventory.title')}
    data-panel="inventory"
    bind:this={panelEl}
    use:draggablePanel={'inventory'}
  >
    <div class="panel-header" data-drag-handle>
      <span class="panel-title">{$t('inventory.title')}</span>
      <span class="gold-display"><GoldAmount copper={$playerGold} /></span>
      <div class="mode-buttons">
        <button
          class="mode-btn select-btn"
          class:active={selectMode}
          aria-pressed={selectMode}
          title={$t('inventory.selectHint')}
          onclick={toggleSelectMode}
        >
          {selectMode ? $t('common.cancel') : $t('inventory.select')}
        </button>
        <button
          class="mode-btn lock-btn"
          class:active={$itemLockMode}
          aria-pressed={$itemLockMode}
          title={$itemLockMode
            ? $t('inventory.lockDone')
            : $t('inventory.lockEdit')}
          onclick={toggleLockMode}>{$t('common.lock')}</button
        >
      </div>
      <button
        class="close-btn"
        aria-label={$t('common.close')}
        onclick={onClose}>&times;</button
      >
    </div>

    <div class="bag-grid">
      {#each slots as slot, i (slot?.instance_id ?? `empty-${i}`)}
        {@const def = slot ? getItemDef(slot.item_def_id) : null}
        {@const locked = selectMode && slot !== null && !isSelectable(slot)}
        {@const onTable = slot
          ? reservedQuantity($playerTrade, slot.instance_id)
          : 0}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="grid-cell"
          class:selected={slot !== null && selected.has(slot.instance_id)}
          class:locked
          class:on-table={onTable > 0}
          use:itemTooltip={def && slot
            ? { def, item: slot, side: 'left' }
            : null}
          ondblclick={() => onDblClick(slot)}
          onpointerdown={(e: PointerEvent) => {
            if (slot && !locked) onPointerDown(e, slot)
          }}
        >
          {#if def}
            <img
              class="item-icon"
              src="/items/{def.icon}"
              alt=""
              draggable="false"
            />
          {/if}
          {#if slot && slot.enchant > 0}
            <span class="item-enchant">+{slot.enchant}</span>
          {/if}
          {#if slot && ($itemLockMode || slot.locked)}
            <ItemLockButton
              item={slot}
              getInstanceIds={() => lockTargets(slot)}
            />
          {/if}
          {#if slot && slot.quantity > 1}
            <span class="item-qty">{slot.quantity}</span>
          {/if}
          {#if onTable > 0}
            <span class="item-reserved" title={$t('inventory.onTrade')}
              >{onTable}</span
            >
          {/if}
          {#if slot !== null && selected.has(slot.instance_id)}
            <span class="item-selected-check">&check;</span>
          {/if}
        </div>
      {/each}
    </div>

    <WeightBar
      current={$carryWeight}
      max={maxWeight}
      label={$t('inventory.weight')}
    />
  </div>
{/if}

<QuantityPopup
  visible={pendingDrop !== null}
  itemName={pendingDrop
    ? itemDisplayName(pendingDrop.def.id, pendingDrop.slot.enchant, $locale)
    : ''}
  icon={pendingDrop?.def.icon ?? ''}
  max={pendingDrop?.slot.quantity ?? 1}
  onConfirm={confirmPendingDrop}
  onCancel={cancelPendingDrop}
/>

<style>
  .inventory-panel {
    --inventory-slot-size: 64px;
    --inventory-slot-gap: 6px;
    --inventory-visible-rows: 10;
    position: fixed;
    right: 9px;
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
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 12px;
    pointer-events: auto;
    max-width: calc(100vw - 32px);
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

  .gold-display {
    font-size: 11px;
    font-weight: 700;
    color: #ffd700;
  }

  .mode-buttons {
    display: flex;
    gap: 4px;
  }

  .mode-btn {
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: #9fb2c3;
    font-family: inherit;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    padding: 2px 6px;
  }

  .mode-btn:hover {
    color: #fff;
    border-color: rgba(255, 255, 255, 0.4);
  }

  .select-btn.active {
    background: rgba(120, 60, 60, 0.85);
    border-color: rgba(220, 140, 140, 0.45);
    color: #fff;
  }

  .lock-btn.active {
    background: #604b20;
    border-color: #d2a63c;
    color: #fff0c5;
  }

  .bag-grid {
    display: grid;
    grid-template-columns: repeat(5, var(--inventory-slot-size));
    grid-auto-rows: var(--inventory-slot-size);
    gap: var(--inventory-slot-gap);
    max-height: calc(
      var(--inventory-slot-size) * var(--inventory-visible-rows) +
        var(--inventory-slot-gap) * (var(--inventory-visible-rows) - 1)
    );
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    scrollbar-width: thin;
    scrollbar-color: rgba(113, 128, 150, 0.5) transparent;
  }

  .bag-grid::-webkit-scrollbar {
    width: 8px;
  }

  .bag-grid::-webkit-scrollbar-track {
    background: transparent;
  }

  .bag-grid::-webkit-scrollbar-thumb {
    background: rgba(113, 128, 150, 0.5);
    border-radius: 999px;
  }

  .bag-grid::-webkit-scrollbar-thumb:hover {
    background: rgba(160, 174, 192, 0.7);
  }

  .grid-cell {
    position: relative;
    box-sizing: border-box;
    width: var(--inventory-slot-size);
    height: var(--inventory-slot-size);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
  }

  .grid-cell.selected {
    border-color: rgba(220, 140, 140, 0.7);
    background: rgba(120, 60, 60, 0.25);
  }

  .grid-cell.locked {
    opacity: 0.35;
    pointer-events: none;
  }

  .item-selected-check {
    position: absolute;
    bottom: 3px;
    left: 4px;
    font-size: 12px;
    font-weight: 700;
    color: #f0b8b8;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.8);
  }

  .item-icon {
    /* Slightly inset and centred so edge-to-edge icons (sword, spear) stay
       inside the slot's border instead of spilling over it. */
    position: absolute;
    inset: 0;
    margin: auto;
    width: 90%;
    height: 90%;
    object-fit: contain;
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

  /* How many units are on the trade table. The slot keeps its place — a bag
     that re-flows mid-trade is a bag you misclick. */
  .grid-cell.on-table {
    box-shadow: inset 0 0 0 1px rgba(210, 170, 80, 0.7);
  }

  .item-reserved {
    position: absolute;
    top: 2px;
    right: 4px;
    font-size: 10px;
    font-weight: 700;
    color: #f0d68a;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.9);
  }

  .item-enchant {
    left: 4px;
  }

  @media (max-width: 600px), (pointer: coarse) {
    .inventory-panel {
      --inventory-slot-size: 48px;
      --inventory-slot-gap: 5px;
      --inventory-visible-rows: 3;
      right: calc(8px + env(safe-area-inset-right));
      top: auto;
      bottom: calc(72px + env(safe-area-inset-bottom));
      transform: none;
      padding: 8px;
      border-radius: 8px;
    }

    .panel-header {
      padding-bottom: 6px;
      margin-bottom: 6px;
      gap: 8px;
    }

    .panel-title {
      font-size: 13px;
    }

    .close-btn {
      min-width: 32px;
      min-height: 32px;
      font-size: 22px;
    }

    .bag-grid {
      -webkit-overflow-scrolling: touch;
    }
  }

  @media (max-width: 340px) {
    .inventory-panel {
      --inventory-slot-size: 44px;
      --inventory-slot-gap: 4px;
    }
  }
</style>
