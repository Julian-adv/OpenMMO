<script lang="ts">
  import { inventoryStore } from '../stores/inventoryStore'
  import type { EquipSlot } from '../stores/inventoryStore'
  import { getItemName } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import type { CharacterAttributes } from '../network/networkTypes'
  import { xpForLevel, clamp } from '../utils/xp'

  interface Props {
    visible: boolean
    level: number
    currentXp: number
    currentHp: number
    maxHp: number
    attributes: CharacterAttributes
  }

  let { visible, level, currentXp, currentHp, maxHp, attributes }: Props = $props()

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

  const levelStartXp = $derived(xpForLevel(level))
  const nextLevelXp = $derived(xpForLevel(level + 1))
  const neededXp = $derived(Math.max(1, nextLevelXp - levelStartXp))
  const gainedXp = $derived(clamp(currentXp - levelStartXp, 0, neededXp))
  const expProgress = $derived(gainedXp / neededXp)
  const expPercent = $derived(Math.round(expProgress * 100))

  function unequip(slot: EquipSlot) {
    networkManager.sendUnequipItem(slot)
  }
</script>

{#if visible}
  <div class="character-panel" role="dialog" aria-label="Character">
    <div class="panel-header">
      <span class="panel-title">Character</span>
    </div>

    <div class="panel-section">
      <div class="section-label">Stats</div>
      <div class="stats-grid">
        <div class="stat-row">
          <span class="stat-label">Lv</span>
          <span class="stat-value level-value">{level}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">HP</span>
          <span class="stat-value hp-value">{currentHp}/{maxHp}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Guard</span>
          <span class="stat-value guard-value">{attributes.guard}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Str</span>
          <span class="stat-value">{attributes.str}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Dex</span>
          <span class="stat-value">{attributes.dex}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Con</span>
          <span class="stat-value">{attributes.con}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Int</span>
          <span class="stat-value">{attributes.int}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Wis</span>
          <span class="stat-value">{attributes.wis}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Cha</span>
          <span class="stat-value">{attributes.cha}</span>
        </div>
      </div>
      <div class="exp-block">
        <div class="exp-header">
          <span class="stat-label exp-label">Exp</span>
          <span class="exp-text">{gainedXp}/{neededXp} ({expPercent}%)</span>
        </div>
        <div class="exp-track" role="progressbar" aria-valuemin={0} aria-valuemax={neededXp} aria-valuenow={gainedXp}>
          <span class="exp-fill" style={`width: ${Math.min(100, expProgress * 100)}%`}></span>
        </div>
      </div>
    </div>

    <div class="panel-section">
      <div class="section-label">Equipment</div>
      <div class="equip-grid">
        {#each EQUIP_SLOTS as slot (slot)}
          {@const item = $inventoryStore.equipped[slot]}
          <div class="equip-slot">
            <span class="slot-label">{EQUIP_SLOT_LABELS[slot]}</span>
            {#if item}
              <span class="slot-item">{getItemName(item.item_def_id)}</span>
              <button class="btn-unequip" onclick={() => unequip(slot)}>X</button>
            {:else}
              <span class="slot-empty">-</span>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .character-panel {
    position: fixed;
    left: 16px;
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

  .stats-grid {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 2px;
    margin-bottom: 8px;
  }

  .stat-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.04);
  }

  .stat-label {
    font-size: 10px;
    color: #9fb2c3;
    min-width: 34px;
  }

  .stat-value {
    font-size: 13px;
    font-weight: 700;
    color: #f5f9fc;
  }

  .level-value {
    color: #f0c040;
  }

  .hp-value {
    color: #6ee7b7;
  }

  .guard-value {
    color: #a78bfa;
  }

  .exp-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .exp-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  .exp-label {
    color: #9fc5ff;
  }

  .exp-text {
    font-size: 11px;
    color: #d5e5f6;
  }

  .exp-track {
    position: relative;
    height: 7px;
    border-radius: 999px;
    overflow: hidden;
    background: rgba(64, 98, 135, 0.45);
    border: 1px solid rgba(166, 200, 238, 0.25);
  }

  .exp-fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: linear-gradient(90deg, #58a6ff 0%, #7fd0ff 100%);
    box-shadow: 0 0 10px rgba(88, 166, 255, 0.4);
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
</style>
