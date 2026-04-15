<script lang="ts">
  import { inventoryStore } from '../stores/inventoryStore'
  import type { EquipSlot } from '../stores/inventoryStore'
  import { getItemDef } from '../data/itemDefs'
  import { networkManager } from '../network/socket'
  import type { CharacterAttributes, CharacterClass, Gender } from '../network/networkTypes'
  import { xpForLevel, clamp } from '../utils/xp'
  import { dragMeta, startDrag, isSlotCompatible, pointInRect, isOverAnyDialog, FALLBACK_ICON } from '../stores/dragStore'
  import ItemTooltip from './ItemTooltip.svelte'

  interface Props {
    visible: boolean
    name: string
    characterClass: CharacterClass
    level: number
    currentXp: number
    currentHp: number
    maxHp: number
    gender: Gender
    attributes: CharacterAttributes
    onClose: () => void
  }

  let { visible, name, characterClass, gender, level, currentXp, currentHp, maxHp, attributes, onClose }: Props = $props()

  const FEMALE_EQUIP_BG: Partial<Record<CharacterClass, string>> = {
    caveman: '/character_concepts/cavewoman.png',
    rogue: '/character_concepts/female_rogue.png',
  }
  const equipBg = $derived(
    (gender === 'female' && FEMALE_EQUIP_BG[characterClass]) ||
      '/character_concepts/female_priest.png'
  )

  const CLASS_LABELS: Record<CharacterClass, string> = {
    knight: 'Knight',
    barbarian: 'Barbarian',
    rogue: 'Rogue',
    caveman: 'Caveman',
    valkyrie: 'Valkyrie',
    ranger: 'Ranger',
    priest: 'Priest',
    merchant: 'Merchant',
    guard: 'Guard',
  }

  const classLabel = $derived(
    characterClass === 'caveman' && gender === 'female'
      ? 'Cavewoman'
      : CLASS_LABELS[characterClass]
  )

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
    ring: 'Ring R',
    ring_left: 'Ring L',
  }

  const SLOT_POSITIONS: { slot: EquipSlot; top: number; left: number }[] = [
    { slot: 'head', top: 9, left: 50 },
    { slot: 'ear', top: 20, left: 70 },
    { slot: 'neck', top: 20, left: 30 },
    { slot: 'chest', top: 30, left: 50 },
    { slot: 'main_hand', top: 45, left: 10 },
    { slot: 'off_hand', top: 45, left: 90 },
    { slot: 'ring', top: 59, left: 10 },
    { slot: 'ring_left', top: 59, left: 90 },
    { slot: 'belt', top: 45, left: 50 },
    { slot: 'pants', top: 60, left: 50 },
    { slot: 'boots', top: 88, left: 50 },
  ]

  let hoveredSlot = $state<EquipSlot | null>(null)

  const levelStartXp = $derived(xpForLevel(level))
  const nextLevelXp = $derived(xpForLevel(level + 1))
  const neededXp = $derived(Math.max(1, nextLevelXp - levelStartXp))
  const gainedXp = $derived(clamp(currentXp - levelStartXp, 0, neededXp))
  const expProgress = $derived(gainedXp / neededXp)
  const expPercent = $derived(Math.round(expProgress * 100))

  function unequip(slot: EquipSlot) {
    networkManager.sendUnequipItem(slot)
  }

  function onEquipPointerDown(e: PointerEvent, slot: EquipSlot, item: { instance_id: number; item_def_id: string }) {
    if (e.button !== 0) return
    e.preventDefault()
    const def = getItemDef(item.item_def_id)

    startDrag(
      e,
      {
        instanceId: item.instance_id,
        equipSlot: def?.equipSlot ?? null,
        source: { type: 'equipped', slot },
        icon: def?.icon ?? FALLBACK_ICON,
      },
      (x, y) => {
        const invPanel = document.querySelector('[data-panel="inventory"]')
        if (invPanel && pointInRect(x, y, invPanel.getBoundingClientRect())) {
          networkManager.sendUnequipItem(slot)
          return
        }
        if (!isOverAnyDialog(x, y)) {
          networkManager.sendDropItem(item.instance_id)
        }
      },
    )
  }
</script>

{#if visible}
  <div class="character-panel" role="dialog" aria-label="Character">
    <div class="panel-header">
      <span class="panel-title">{name}</span>
      <span class="panel-class">{classLabel}</span>
      <button class="close-btn" onclick={onClose}>&times;</button>
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

    <div class="panel-section equip-section">
      <img class="equip-bg" src={equipBg} alt="" draggable="false" />
      {#each SLOT_POSITIONS as { slot, top, left } (slot)}
        {@const item = $inventoryStore.equipped[slot]}
        {@const def = item ? getItemDef(item.item_def_id) : null}
        {@const isDropTarget = $dragMeta && isSlotCompatible($dragMeta.equipSlot, slot)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="equip-slot"
          class:drop-target={isDropTarget}
          style="top:{top}%;left:{left}%"
          title={item ? undefined : EQUIP_SLOT_LABELS[slot]}
          data-equip-slot={slot}
          onmouseenter={() => { if (item) hoveredSlot = slot }}
          onmouseleave={() => hoveredSlot = null}
          ondblclick={() => { if (item) unequip(slot) }}
          onpointerdown={(e: PointerEvent) => { if (item) onEquipPointerDown(e, slot, item) }}
        >
          {#if def}
            <img class="equip-icon" src="/items/{def.icon}" alt={def.name} draggable="false" />
          {/if}
          {#if item && def && hoveredSlot === slot && !$dragMeta}
            <ItemTooltip {def} side={left > 50 ? 'left' : 'right'} />
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .character-panel {
    position: fixed;
    left: 16px;
    top: 45%;
    transform: translateY(-50%);
    z-index: 40;
    width: 364px;
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

  .panel-class {
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

  .equip-section {
    position: relative;
    border-radius: 6px;
    min-height: 540px;
  }

  .equip-bg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    object-position: center bottom;
    opacity: 0.4;
    pointer-events: none;
  }

  .equip-slot {
    position: absolute;
    width: 64px;
    height: 64px;
    transform: translate(-50%, -50%);
    border: 1px solid rgba(255, 255, 255, 0.3);
    border-radius: 6px;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }

  .equip-slot:hover {
    border-color: rgba(240, 192, 64, 0.6);
    background: rgba(240, 192, 64, 0.08);
    z-index: 10;
  }

  .equip-slot.drop-target {
    border-color: rgba(88, 255, 88, 0.8);
    background: rgba(88, 255, 88, 0.15);
    box-shadow: 0 0 8px rgba(88, 255, 88, 0.4);
  }

  .equip-icon {
    width: 56px;
    height: 56px;
    image-rendering: pixelated;
    pointer-events: none;
  }

</style>
