<script lang="ts">
  import { locale } from '../i18n'
  import { onMount } from 'svelte'
  import {
    landscapingMode,
    landscapingPending,
    landscapingError,
    landscapingHint,
    hasLandscapingToolbox,
    selectLandscapingTool,
  } from '../stores/landscapingStore'
  import {
    fenceMode,
    fencePending,
    fenceError,
    fenceTarget,
    fenceCount,
    showFenceNoSpawnZones,
    stopFenceMode,
  } from '../stores/fenceStore'
  import {
    estateChestError,
    estateChestMode,
    estateChestPending,
    stopEstateChestMode,
  } from '../stores/estateStorageStore'
  import { inventoryStore } from '../stores/inventoryStore'
  import {
    ESTATE_FURNITURE_HEIGHT_STEP,
    estateFurniturePlacementHeight,
    estateFurniturePlacementRotation,
    setEstateFurniturePlacementHeight,
    adjustEstateFurniturePlacementHeight,
    rotateEstateFurniturePlacement,
    selectedEstateFurniture,
    estateFurnitureCatalogOpen,
    showEstateFurnitureCatalog,
    estateFurnitureSelectionMode,
    estateFurnitureEditorActive,
    startEstateFurnitureSelection,
    beginEstateFurnitureRecovery,
    beginEstateFurniturePlacementSave,
  } from '../stores/estateFurniturePlacementStore'
  import { estateStorageDefs } from '../data/estateFurnitureDefs'
  import { getItemDef, itemDisplayName } from '../data/itemDefs'
  import { playerVisualFloorLevel } from '../stores/housingStore'
  import { networkManager } from '../network/socket'
  import type { LandscapingTool } from '../terrain/landscaping'
  import { isAdminUser } from '../stores/gameStore'
  import { stopHouseInteraction } from '../stores/housePlacementStore'
  import HousePlacementPanel from './HousePlacementPanel.svelte'
  import SplatBrushPanel from './map-editor/SplatBrushPanel.svelte'
  import { draggablePanel } from '../actions/draggablePanel'
  import { isTypingTarget } from '../utils/dom'

  type EditorTab = Exclude<LandscapingTool, 'Fence'> | 'Objects'

  let panelElement = $state<HTMLDivElement>()
  const selectedFurnitureId = $derived($selectedEstateFurniture?.id)
  const selectedSignId = $derived(
    estateStorageDefs.get($selectedEstateFurniture?.item_def_id ?? '')
      ?.textLabel
      ? selectedFurnitureId
      : undefined
  )
  const savedSignText = $derived($selectedEstateFurniture?.text ?? '')
  let signText = $derived(selectedSignId === undefined ? '' : savedSignText)
  $effect(() => {
    if (selectedFurnitureId !== undefined && panelElement)
      panelElement.scrollTop = 0
  })

  const tabs: EditorTab[] = ['Ground', 'Road', 'Objects', 'House']
  const editorOpen = $derived(
    $landscapingMode !== null ||
      $estateFurnitureEditorActive ||
      $estateFurnitureCatalogOpen
  )
  const activeTab = $derived<EditorTab>(
    $estateFurnitureEditorActive ||
      $estateFurnitureCatalogOpen ||
      $landscapingMode?.tool === 'Fence'
      ? 'Objects'
      : ($landscapingMode?.tool ?? 'Objects')
  )
  const fenceDefinition = getItemDef('wooden_fence')
  const rotationStep = $derived(
    estateStorageDefs.get($estateChestMode?.item_def_id ?? '')?.rotationStep ??
      90
  )
  const maxHeightOffset = $derived(
    estateStorageDefs.get($estateChestMode?.item_def_id ?? '')
      ?.maxHeightOffset ?? 0
  )
  const storageObjects = $derived(
    [...estateStorageDefs.values()]
      .map((definition) => {
        const items = $inventoryStore.bag.filter(
          (item) => item.item_def_id === definition.itemDefId
        )
        return {
          definition: getItemDef(definition.itemDefId),
          itemDefId: definition.itemDefId,
          instanceId: items[0]?.instance_id,
          quantity: items.reduce((total, item) => total + item.quantity, 0),
        }
      })
      .filter((object) => object.quantity > 0)
  )

  function selectTab(tab: EditorTab) {
    if (tab === activeTab || $estateChestPending) return
    if (tab === 'Objects') {
      showObjects()
      return
    }
    if (tab !== 'House') {
      stopHouseInteraction()
    }
    if ($estateFurnitureEditorActive || $estateFurnitureCatalogOpen) {
      stopEstateChestMode()
      networkManager.sendStartLandscapingMode(tab)
    } else selectLandscapingTool(tab)
  }

  function showObjects() {
    if ($estateChestPending) return
    stopHouseInteraction()
    stopFenceMode()
    showEstateFurnitureCatalog()
  }

  function selectObjects() {
    if ($estateChestPending) return
    stopHouseInteraction()
    stopFenceMode()
    startEstateFurnitureSelection()
  }

  function selectFence() {
    if ($fenceMode || $estateChestPending) return
    stopHouseInteraction()
    stopEstateChestMode()
    if ($landscapingMode) {
      selectLandscapingTool('Fence')
    } else {
      networkManager.sendStartLandscapingMode('Fence')
    }
  }

  function selectStorage(instanceId: number | undefined) {
    if (instanceId === undefined || $estateChestPending) return
    stopHouseInteraction()
    networkManager.sendUseItem(instanceId)
  }

  function recoverSelected() {
    const furnitureId = beginEstateFurnitureRecovery()
    if (furnitureId !== null) networkManager.sendRecoverEstateChest(furnitureId)
  }

  function saveSignText() {
    if (
      selectedSignId === undefined ||
      $estateChestMode?.kind !== 'move' ||
      !beginEstateFurniturePlacementSave()
    )
      return
    networkManager.sendSetEstateFurnitureText(selectedSignId, signText.trim())
  }

  function adjustHeightWithWheel(event: WheelEvent) {
    event.preventDefault()
    event.stopPropagation()
    if (event.deltaY !== 0)
      adjustEstateFurniturePlacementHeight(event.deltaY < 0 ? 1 : -1)
  }

  function close() {
    if ($estateChestPending) return
    stopHouseInteraction()
    stopFenceMode()
    stopEstateChestMode()
  }

  onMount(() => {
    const blockInactiveEnter = (event: KeyboardEvent) => {
      if (
        (event.code !== 'Enter' && event.code !== 'NumpadEnter') ||
        $estateChestMode ||
        (!$estateFurnitureSelectionMode &&
          !$selectedEstateFurniture &&
          !($estateFurnitureCatalogOpen && event.repeat)) ||
        (isTypingTarget(event.target) &&
          !(
            event.target instanceof HTMLInputElement &&
            event.target.type === 'range'
          ))
      )
        return
      event.preventDefault()
      event.stopImmediatePropagation()
    }
    window.addEventListener('keydown', blockInactiveEnter, true)
    return () => window.removeEventListener('keydown', blockInactiveEnter, true)
  })
</script>

{#if editorOpen}
  {@const status = $estateFurnitureEditorActive
    ? $estateChestPending
      ? 'Working…'
      : ($estateChestError ??
        ($estateChestMode
          ? 'Point inside your estate and click to place'
          : $selectedEstateFurniture
            ? 'Select the furniture again to move or rotate it'
            : 'Left-click nearby furniture to move or rotate it'))
    : $landscapingMode?.tool === 'House'
      ? null
      : $landscapingMode?.tool === 'Fence'
        ? $fencePending
          ? 'Saving…'
          : ($fenceError ?? $fenceTarget?.reason)
        : $landscapingPending
          ? 'Saving…'
          : ($landscapingError ?? $landscapingHint)}
  <div
    class="landscaping-panel"
    class:objects-panel={activeTab === 'Objects'}
    bind:this={panelElement}
    use:draggablePanel={'landscaping'}
  >
    <div class="panel-header" data-drag-handle>
      <strong>Estate Editor</strong>
      <button
        class="close-btn"
        disabled={$estateChestPending}
        aria-label="Close estate editor"
        title="Close (Esc)"
        onclick={close}>×</button
      >
    </div>
    <div class="tabs" role="tablist" aria-label="Estate editing tools">
      {#each tabs as tab (tab)}
        <button
          role="tab"
          aria-selected={activeTab === tab}
          class:active={activeTab === tab}
          disabled={$estateChestPending ||
            (tab !== 'Objects' && !$hasLandscapingToolbox)}
          title={tab !== 'Objects' && !$hasLandscapingToolbox
            ? "Carry a Landscaper's Toolbox to use this tool"
            : tab}
          onclick={() => selectTab(tab)}>{tab}</button
        >
      {/each}
    </div>
    {#if activeTab === 'House'}
      <HousePlacementPanel />
    {:else if activeTab === 'Objects'}
      <div class="object-content">
        <button
          class="select-objects"
          class:active={$estateFurnitureSelectionMode}
          aria-pressed={$estateFurnitureSelectionMode}
          disabled={$estateChestPending}
          title="Select placed furniture to move, rotate, or recover"
          onclick={selectObjects}>Select</button
        >
        {#if $estateFurnitureSelectionMode && !$selectedEstateFurniture}
          <small
            >Left-click furniture on this floor to start moving or rotating it.
            Esc closes the editor.</small
          >
          <button disabled={$estateChestPending} onclick={showObjects}
            >Done</button
          >
        {/if}
        {#if $selectedEstateFurniture}
          {@const selectedDefinition = getItemDef(
            $selectedEstateFurniture.item_def_id
          )}
          <div class="selected-furniture">
            <div class="selected-furniture-name">
              {#if selectedDefinition}
                <img src="/items/{selectedDefinition.icon}" alt="" />
              {/if}
              <strong
                >{itemDisplayName(
                  $selectedEstateFurniture.item_def_id,
                  0,
                  $locale
                )}</strong
              >
            </div>
            {#if $estateChestMode?.kind === 'move'}
              <small>Left-click or Enter to save each adjustment.</small>
            {/if}
            {#if selectedSignId !== undefined}
              <div class="sign-text-controls">
                <label for="estate-sign-text">Sign text</label>
                <textarea
                  id="estate-sign-text"
                  bind:value={signText}
                  maxlength="120"
                  rows="3"
                  disabled={$estateChestPending}
                  onkeydown={(event) => event.stopPropagation()}></textarea>
                <button
                  disabled={$estateChestPending ||
                    $estateChestMode?.kind !== 'move' ||
                    signText.trim() === savedSignText}
                  onclick={saveSignText}>Save text</button
                >
              </div>
            {/if}
            <div class="furniture-actions">
              <button disabled={$estateChestPending} onclick={recoverSelected}
                >Recover</button
              >
              <button disabled={$estateChestPending} onclick={showObjects}
                >Done</button
              >
            </div>
          </div>
        {/if}
        {#if $estateChestMode}
          <div class="rotation-controls">
            <button
              disabled={$estateChestPending}
              aria-label="Rotate left {rotationStep} degrees"
              title="Rotate left (Shift + R)"
              onclick={() => rotateEstateFurniturePlacement(-1)}
              >↶ {rotationStep}°</button
            >
            <span>Rotation: {$estateFurniturePlacementRotation.degrees}°</span>
            <button
              disabled={$estateChestPending}
              aria-label="Rotate right {rotationStep} degrees"
              title="Rotate right (R)"
              onclick={() => rotateEstateFurniturePlacement(1)}
              >↷ {rotationStep}°</button
            >
          </div>
          {#if maxHeightOffset > 0}
            <div class="height-controls">
              <label for="estate-furniture-height">Height (Y)</label>
              <output for="estate-furniture-height"
                >{($estateFurniturePlacementHeight.offset ?? 0).toFixed(2)} m</output
              >
              <input
                id="estate-furniture-height"
                type="range"
                min="0"
                max={maxHeightOffset}
                step={ESTATE_FURNITURE_HEIGHT_STEP}
                value={$estateFurniturePlacementHeight.offset ?? 0}
                disabled={$estateChestPending}
                title="Height above the floor. Scroll here or in the scene to adjust."
                oninput={(event) =>
                  setEstateFurniturePlacementHeight(
                    event.currentTarget.valueAsNumber
                  )}
                onwheel={adjustHeightWithWheel}
              />
              <small>Above floor · Mouse wheel adjusts height</small>
            </div>
          {/if}
        {/if}
        {#if !$estateFurnitureSelectionMode && !$selectedEstateFurniture}
          <strong>Placeable Objects</strong>
          <div class="object-list">
            <button
              class="object-row"
              class:active={$fenceMode !== null}
              title={$fenceCount
                ? 'Place or recover fences'
                : 'Select to recover placed fences'}
              onclick={selectFence}
            >
              {#if fenceDefinition}
                <img src="/items/{fenceDefinition.icon}" alt="" />
              {/if}
              <span>{itemDisplayName('wooden_fence', 0, $locale)}</span>
              <small>×{$fenceCount}</small>
            </button>
            {#each storageObjects as object (object.itemDefId)}
              <button
                class="object-row"
                class:active={$estateChestMode?.item_def_id ===
                  object.itemDefId}
                disabled={object.instanceId === undefined ||
                  $estateChestPending}
                title={`Place ${itemDisplayName(object.itemDefId, 0, $locale)}`}
                onclick={() => selectStorage(object.instanceId)}
              >
                {#if object.definition}
                  <img src="/items/{object.definition.icon}" alt="" />
                {/if}
                <span>{itemDisplayName(object.itemDefId, 0, $locale)}</span>
                <small>×{object.quantity}</small>
              </button>
            {/each}
          </div>
        {/if}
        {#if $isAdminUser && $fenceMode}
          <label class="zone-toggle">
            <input type="checkbox" bind:checked={$showFenceNoSpawnZones} />
            Show no-spawn zones
          </label>
        {/if}
        {#if $estateChestMode}
          <small
            >{$playerVisualFloorLevel + 1}F · {$estateChestMode.kind === 'move'
              ? 'Left-click or Enter to save'
              : 'Left-click or Enter to place'} · ↑ / ↓ north / south · ← / → west
            / east (5 cm) · R / Shift + R rotates {rotationStep}° · Mouse wheel
            adjusts decoration height · Ctrl + wheel in the scene zooms the
            camera · Select to choose another object · Esc to finish</small
          >
        {:else if $fenceMode}
          <small>Left-click to place or recover · Esc to finish</small>
        {/if}
        {#if !$estateChestMode && !$estateFurnitureSelectionMode}
          <small>Use Select, then left-click placed furniture to edit it.</small
          >
        {/if}
      </div>
    {:else}
      <SplatBrushPanel
        sizeLabel={activeTab === 'Road' ? 'Width' : 'Size'}
        title={activeTab === 'Ground' ? 'Ground Brush' : 'Road Tool'}
        hint={activeTab === 'Ground' ? '(drag to paint)' : '(click two points)'}
        availableLayers={$landscapingMode?.palette ?? []}
      />
    {/if}
    {#if status}
      <div class="paint-status" role="status">{status}</div>
    {/if}
  </div>
{/if}

<style>
  .landscaping-panel {
    position: fixed;
    bottom: 100px;
    left: 16px;
    z-index: 40;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 120px);
    overflow: auto;
    scrollbar-width: thin;
    scrollbar-color: rgba(171, 147, 103, 0.5) transparent;
    border-radius: 8px;
    background: #211c16ed;
    color: #f3e8d2;
    pointer-events: auto;
  }
  .landscaping-panel.objects-panel {
    width: 360px;
  }
  .landscaping-panel::-webkit-scrollbar {
    width: 6px;
    height: 6px;
  }
  .landscaping-panel::-webkit-scrollbar-track {
    background: transparent;
  }
  .landscaping-panel::-webkit-scrollbar-thumb {
    background: rgba(171, 147, 103, 0.5);
    border-radius: 999px;
  }
  .landscaping-panel::-webkit-scrollbar-thumb:hover {
    background: rgba(205, 178, 128, 0.7);
  }
  .landscaping-panel::-webkit-scrollbar-corner {
    background: transparent;
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 8px;
    font-family: 'Courier New', monospace;
    font-size: 13px;
  }
  button {
    cursor: pointer;
    color: inherit;
    background: #3a3024;
    border: 1px solid #766247;
    border-radius: 4px;
    padding: 5px 14px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    font-weight: bold;
  }
  .close-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    background: transparent;
    font-size: 16px;
    line-height: 1;
  }
  .close-btn:hover {
    background: #3a3024;
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .tabs {
    display: flex;
    gap: 2px;
    margin: 0 0 4px;
    padding: 2px;
    background: rgba(0, 0, 0, 0.7);
  }
  .tabs button {
    flex: 1;
    padding: 4px 10px;
    border: none;
    background: transparent;
    color: #888;
    letter-spacing: 0.5px;
    transition:
      background 150ms ease,
      color 150ms ease;
  }
  .tabs button:hover:not(:disabled) {
    color: #ccc;
  }
  .tabs button.active {
    background: rgba(226, 185, 59, 0.25);
    color: #e2b93b;
  }
  .landscaping-panel :global(.splat-brush-panel) {
    border: none;
    border-block: 1px solid rgba(226, 185, 59, 0.3);
    border-radius: 0;
    box-shadow: none;
  }
  .object-content,
  .paint-status {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
    padding: 12px 16px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .object-list {
    display: grid;
    grid-auto-rows: minmax(42px, max-content);
    gap: 5px;
    min-width: 0;
    max-height: 324px;
    overflow-y: auto;
    scrollbar-width: thin;
  }
  .selected-furniture {
    display: grid;
    gap: 8px;
    padding: 10px;
    border: 1px solid rgba(226, 185, 59, 0.3);
    border-radius: 6px;
    background: rgba(226, 185, 59, 0.08);
  }
  .selected-furniture small {
    max-width: 300px;
    color: #aaa;
  }
  .selected-furniture-name {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .selected-furniture-name img {
    width: 28px;
    height: 28px;
    flex-shrink: 0;
    object-fit: contain;
  }
  .furniture-actions {
    display: flex;
    gap: 6px;
  }
  .sign-text-controls {
    display: grid;
    gap: 6px;
    min-width: 0;
  }
  .sign-text-controls textarea {
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    resize: vertical;
    padding: 6px;
    border: 1px solid #766247;
    border-radius: 4px;
    background: #211c16;
    color: inherit;
    font: inherit;
  }
  .rotation-controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .object-row {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    text-align: left;
  }
  .height-controls {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px;
  }
  .height-controls input,
  .height-controls small {
    grid-column: 1 / -1;
  }
  .height-controls input {
    width: 100%;
    min-width: 0;
    margin: 0;
    accent-color: #e2b93b;
  }
  .height-controls small {
    color: #aaa;
  }
  .object-row.active,
  .select-objects.active {
    border-color: #e2b93b;
    background: rgba(226, 185, 59, 0.2);
  }
  .object-row img {
    width: 28px;
    height: 28px;
    object-fit: contain;
  }
  .object-row small,
  .object-content > small {
    color: #aaa;
  }
  .zone-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .zone-toggle input {
    accent-color: #e2b93b;
  }
</style>
