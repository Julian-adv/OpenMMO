<script lang="ts">
  import { locale } from '../i18n'
  import { itemDisplayName } from '../data/itemDefs'
  import SkillTargetHint from './SkillTargetHint.svelte'
  import { AUSCULTATION, abilityEquipmentAllowed } from '../data/abilities'
  import { EQUIP_SLOT_LABELS } from '../data/equipSlots'
  import { getItemDef } from '../data/itemDefs'
  import { itemTooltip } from '../actions/itemTooltip'
  import { draggablePanel } from '../actions/draggablePanel'
  import { inventoryStore } from '../stores/inventoryStore'
  import { gameStore } from '../stores/gameStore'
  import {
    teleportLoading,
    mapEditorMode,
    housingEditorMode,
  } from '../stores/debugStore'
  import { landscapingMode } from '../stores/landscapingStore'
  import { estateFurnitureEditorActive } from '../stores/estateFurniturePlacementStore'
  import { isMounted } from '../utils/mounts'
  import { mountOverlay } from '../stores/overlayStack'
  import {
    inspectionTargeting,
    inspectionResult,
    cancelInspection,
    resetInspection,
  } from '../stores/inspectionStore'

  $effect(() => {
    if (!$inspectionTargeting && !$inspectionResult) return
    if (
      !$gameStore.currentPlayer ||
      !$gameStore.isConnected ||
      $gameStore.currentPlayer.health <= 0 ||
      $teleportLoading
    )
      resetInspection()
    else if (
      !abilityEquipmentAllowed(AUSCULTATION.id, $inventoryStore.equipped) ||
      isMounted($gameStore.currentPlayer) ||
      $mapEditorMode ||
      $housingEditorMode ||
      $landscapingMode ||
      $estateFurnitureEditorActive
    )
      cancelInspection()
  })

  $effect(() => {
    if ($inspectionTargeting)
      return mountOverlay('inspectionTarget', cancelInspection)
  })

  function close() {
    inspectionResult.set(null)
  }

  $effect(() => {
    if ($inspectionResult) return mountOverlay('inspection', close)
  })
</script>

{#if $inspectionTargeting}
  <SkillTargetHint
    icon={AUSCULTATION.icon}
    message="Left-click a nearby player or monster. Esc to cancel."
    onCancel={cancelInspection}
  />
{/if}

{#if $inspectionResult}
  {@const result = $inspectionResult}
  <div
    class="inspection-panel"
    role="dialog"
    aria-label="Auscultation results"
    use:draggablePanel={'inspection'}
  >
    <div class="panel-header" data-drag-handle>
      <span class="panel-title">Auscultation</span>
      <button
        type="button"
        class="close-btn"
        aria-label="Close auscultation results"
        onclick={close}>&times;</button
      >
    </div>
    <div class="content">
      <h2>{result.name}</h2>
      <dl>
        <div>
          <dt>Level</dt>
          <dd class="level-value">{result.level}</dd>
        </div>
        <div>
          <dt>HP</dt>
          <dd class="hp-value">{result.health} / {result.max_health}</dd>
        </div>
        <div>
          <dt>Guard</dt>
          <dd class="guard-value">{result.guard}</dd>
        </div>
      </dl>
      <h3>Equipment</h3>
      {#if result.equipment.length}
        <ul>
          {#each result.equipment as item (item.slot)}
            {@const def = getItemDef(item.item_def_id)}
            <li use:itemTooltip={def ? { def, enchant: item.enchant } : null}>
              {#if def}<img
                  class="item-icon"
                  src="/items/{def.icon}"
                  alt=""
                />{/if}
              <span class="item-name"
                >{itemDisplayName(item.item_def_id, 0, $locale)}{item.enchant
                  ? ` ${item.enchant > 0 ? '+' : ''}${item.enchant}`
                  : ''}</span
              >
              <span class="slot-name">{EQUIP_SLOT_LABELS[item.slot]}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <p>No equipment.</p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .inspection-panel {
    position: fixed;
    top: 18%;
    right: 24px;
    width: min(340px, calc(100vw - 24px));
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    backdrop-filter: blur(4px);
    padding: 10px;
    color: #e6edf3;
    background: rgba(6, 10, 14, 0.88);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    pointer-events: auto;
    font-family: 'Courier New', monospace;
    font-size: 12px;
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 8px;
  }
  .panel-title {
    font-size: 14px;
    font-weight: 700;
    color: #f0c040;
  }
  button {
    cursor: pointer;
    color: inherit;
  }
  .close-btn {
    background: none;
    border: none;
    color: #9fb2c3;
    font-size: 18px;
    padding: 0 2px;
    line-height: 1;
  }
  .close-btn:hover {
    color: #fff;
  }
  .content {
    overflow-y: auto;
  }
  h2 {
    font-size: 14px;
    margin: 0 0 8px;
  }
  h3 {
    font-size: 11px;
    margin: 12px 0 6px;
    color: #9fc5ff;
  }
  dl {
    margin: 0;
    display: grid;
    gap: 2px;
  }
  dl div {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 3px 4px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.04);
  }
  dt {
    font-size: 10px;
    color: #9fb2c3;
  }
  dd {
    margin: 0;
    font-size: 13px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
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
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 36px;
  }
  .item-icon {
    width: 32px;
    height: 32px;
    box-sizing: border-box;
    border: 1px solid rgba(255, 255, 255, 0.3);
    border-radius: 6px;
    background: rgba(148, 156, 164, 0.34);
    object-fit: contain;
  }
  .item-name {
    flex: 1;
  }
  .slot-name {
    color: #9fb2c3;
    font-size: 11px;
  }
</style>
