<script lang="ts">
  import { inventoryStore } from '../stores/inventoryStore'
  import type { EquipSlot, ItemInstance } from '../stores/inventoryStore'
  import { getItemDef } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import type { CharacterAttributes } from '../network/networkTypes'

  interface Props {
    visible: boolean
    attributes: CharacterAttributes | null
  }

  let { visible, attributes }: Props = $props()

  const EQUIP_SLOT_LABELS: Record<EquipSlot, string> = {
    head: 'Head',
    main_hand: 'Main Hand',
    off_hand: 'Off Hand',
    chest: 'Chest',
    ear: 'Ear',
    neck: 'Neck',
    belt: 'Belt',
    pants: 'Pants',
    boots: 'Boots',
    ring: 'Ring',
  }

  const EQUIP_SLOTS: EquipSlot[] = [
    'head',
    'neck',
    'ear',
    'chest',
    'main_hand',
    'off_hand',
    'belt',
    'pants',
    'boots',
    'ring',
  ]

  const maxWeight = $derived(attributes ? attributes.str * 15 : 150)

  function itemWeight(item: ItemInstance): number {
    const def = getItemDef(item.item_def_id)
    return (def?.weight ?? 1) * item.quantity
  }

  function itemName(item: ItemInstance): string {
    return getItemDef(item.item_def_id)?.name ?? item.item_def_id
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

  function equip(instanceId: number) {
    networkManager.sendEquipItem(instanceId)
  }

  function unequip(slot: EquipSlot) {
    networkManager.sendUnequipItem(slot)
  }

  function drop(instanceId: number) {
    networkManager.sendDropItem(instanceId)
  }
</script>

{#if visible}
  <div class="inventory-panel" role="dialog" aria-label="Inventory">
    <div class="panel-header">
      <span class="panel-title">Inventory</span>
      <span class="weight-display">
        {currentWeight.toFixed(1)} / {maxWeight} lbs
      </span>
    </div>

    <div class="panel-section">
      <div class="section-label">Equipment</div>
      <div class="equip-grid">
        {#each EQUIP_SLOTS as slot (slot)}
          {@const item = $inventoryStore.equipped[slot]}
          <div class="equip-slot">
            <span class="slot-label">{EQUIP_SLOT_LABELS[slot]}</span>
            {#if item}
              <span class="slot-item">{itemName(item)}</span>
              <button class="btn-unequip" onclick={() => unequip(slot)}>X</button>
            {:else}
              <span class="slot-empty">-</span>
            {/if}
          </div>
        {/each}
      </div>
    </div>

    <div class="panel-section bag-section">
      <div class="section-label">Bag ({$inventoryStore.bag.length})</div>
      <div class="bag-list">
        {#each $inventoryStore.bag as item (item.instance_id)}
          {@const def = getItemDef(item.item_def_id)}
          <div class="bag-item">
            <span class="item-name">{itemName(item)}</span>
            <span class="item-weight">{(def?.weight ?? 1).toFixed(1)}</span>
            <div class="item-actions">
              {#if def?.equipSlot}
                <button class="btn-action" onclick={() => equip(item.instance_id)}>Equip</button>
              {/if}
              <button class="btn-action btn-drop" onclick={() => drop(item.instance_id)}>Drop</button>
            </div>
          </div>
        {:else}
          <div class="bag-empty">Empty</div>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .inventory-panel {
    position: fixed;
    right: 16px;
    top: 50%;
    transform: translateY(-50%);
    z-index: 40;
    width: 280px;
    max-height: 80vh;
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
    overflow: hidden;
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

  .panel-section {
    margin-bottom: 8px;
  }

  .section-label {
    font-size: 11px;
    color: #9fc5ff;
    margin-bottom: 4px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .equip-grid {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .equip-slot {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.04);
  }

  .slot-label {
    min-width: 70px;
    font-size: 10px;
    color: #9fb2c3;
  }

  .slot-item {
    flex: 1;
    font-size: 11px;
    color: #6ee7b7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .slot-empty {
    flex: 1;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.2);
  }

  .btn-unequip {
    background: none;
    border: none;
    color: #ef4444;
    font-size: 10px;
    cursor: pointer;
    padding: 0 4px;
    font-family: inherit;
  }

  .btn-unequip:hover {
    color: #f87171;
  }

  .bag-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .bag-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 240px;
  }

  .bag-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.04);
  }

  .bag-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .item-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-weight {
    font-size: 10px;
    color: #9fb2c3;
    min-width: 32px;
    text-align: right;
  }

  .item-actions {
    display: flex;
    gap: 2px;
  }

  .btn-action {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 3px;
    color: #e6edf3;
    font-size: 10px;
    padding: 1px 6px;
    cursor: pointer;
    font-family: inherit;
  }

  .btn-action:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .btn-drop {
    color: #ef4444;
    border-color: rgba(239, 68, 68, 0.3);
  }

  .btn-drop:hover {
    background: rgba(239, 68, 68, 0.2);
  }

  .bag-empty {
    color: rgba(255, 255, 255, 0.3);
    padding: 8px;
    text-align: center;
    font-style: italic;
  }
</style>
