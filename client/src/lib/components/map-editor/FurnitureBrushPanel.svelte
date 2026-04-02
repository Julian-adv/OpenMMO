<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import {
    furnitureCatalog,
    selectedFurnitureType,
    furnitureRotation,
    currentFurnitureData,
    selectedFurniturePlacementId,
    furnitureSubTool,
  } from '../../stores/editorStore'
  import type {
    FurnitureDef,
    FurniturePlacement,
    FurnitureRegionData,
    FurnitureSubTool,
  } from '../../stores/editorStore'
  import { furnitureManager } from '../../managers/furnitureManager'
  import { playerFloorLevel } from '../../stores/housingStore'
  import { currentEditorRegion } from '../../stores/editorStore'

  let catalog = $state<FurnitureDef[]>([])
  let selected = $state<string | null>(null)
  let rotation = $state(0)
  let placements = $state<FurniturePlacement[]>([])
  let selectedPlacementId = $state<number | null>(null)
  let subTool = $state<FurnitureSubTool>('place')
  let floor = $state(-1)

  const unsubs = [
    furnitureCatalog.subscribe((v) => (catalog = v)),
    selectedFurnitureType.subscribe((v) => (selected = v)),
    furnitureRotation.subscribe((v) => (rotation = v)),
    currentFurnitureData.subscribe((v) => (placements = v.placements)),
    selectedFurniturePlacementId.subscribe((v) => (selectedPlacementId = v)),
    furnitureSubTool.subscribe((v) => (subTool = v)),
    playerFloorLevel.subscribe((v) => (floor = v)),
  ]
  onDestroy(() => unsubs.forEach((u) => u()))

  onMount(async () => {
    if (get(furnitureCatalog).length > 0) return
    const list = await furnitureManager.fetchCatalog()
    furnitureCatalog.set(list)
  })

  function selectType(id: string) {
    selectedFurnitureType.set(id)
    furnitureSubTool.set('place')
    selectedFurniturePlacementId.set(null)
  }

  function setSubTool(tool: FurnitureSubTool) {
    furnitureSubTool.set(tool)
    if (tool === 'place') {
      selectedFurniturePlacementId.set(null)
    }
  }

  async function deletePlacement() {
    if (selectedPlacementId === null) return
    const data = get(currentFurnitureData)
    const updated: FurnitureRegionData = {
      placements: data.placements.filter((p) => p.id !== selectedPlacementId),
    }
    currentFurnitureData.set(updated)
    selectedFurniturePlacementId.set(null)

    const region = get(currentEditorRegion)
    if (region) {
      await furnitureManager.saveFurniture(region.rx, region.rz, updated)
    }
  }

  function formatPos(p: FurniturePlacement): string {
    return `${p.x.toFixed(1)}, ${p.y.toFixed(1)}, ${p.z.toFixed(1)}`
  }

  let selectedPlacement = $derived(
    placements.find((p) => p.id === selectedPlacementId) ?? null
  )
</script>

<div class="furniture-panel">
  <div class="panel-title">Furniture</div>

  <div class="sub-tools">
    <button
      class="sub-tool-btn"
      class:active={subTool === 'place'}
      onclick={() => setSubTool('place')}
    >Place</button>
    <button
      class="sub-tool-btn"
      class:active={subTool === 'select'}
      onclick={() => setSubTool('select')}
    >Select</button>
  </div>

  {#if subTool === 'place'}
    <div class="section-label">Catalog</div>
    <div class="furniture-list">
      {#each catalog as item (item.id)}
        <button
          class="furniture-item-btn"
          class:active={selected === item.id}
          onclick={() => selectType(item.id)}
        >
          <span class="item-name">{item.name}</span>
          <span class="item-action">{item.interaction}</span>
        </button>
      {/each}
    </div>

    {#if selected}
      <div class="section-label">Placement</div>
      <div class="rotation-display">
        <span class="rotation-value">{rotation}&deg;</span>
        <span class="rotation-hint">Press R to rotate</span>
      </div>
      <div class="rotation-display" style="margin-top: 2px">
        <span class="rotation-value">{floor < 0 ? 'Outside' : `${floor + 1}F`}</span>
        <span class="rotation-hint">Follow player floor</span>
      </div>
    {:else}
      <div class="draw-hint">Select a furniture type to place</div>
    {/if}
  {:else}
    {#if selectedPlacement}
      <div class="section-label">Selected</div>
      <div class="selected-info">
        <div class="coord-row">
          <span class="info-label">Type:</span>
          <span class="info-value">{selectedPlacement.type}</span>
        </div>
        <div class="coord-row">
          <span class="info-label">Pos:</span>
          <span class="info-value">{formatPos(selectedPlacement)}</span>
        </div>
        <div class="coord-row">
          <span class="info-label">Rot:</span>
          <span class="info-value">{selectedPlacement.rotation}&deg;</span>
        </div>
        <div class="coord-row">
          <span class="info-label">Floor:</span>
          <span class="info-value">{selectedPlacement.floorLevel + 1}F</span>
        </div>
        <button class="delete-btn" onclick={deletePlacement}>Delete</button>
      </div>
    {:else}
      <div class="draw-hint">Click a placed furniture to select</div>
    {/if}
  {/if}

  {#if placements.length > 0}
    <div class="section-label">Placed ({placements.length})</div>
    <div class="placement-list">
      {#each placements as p (p.id)}
        <div
          class="placement-row"
          class:active={p.id === selectedPlacementId}
        >
          <span class="placement-type">{p.type}</span>
          <span class="placement-pos">{formatPos(p)}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .furniture-panel {
    background: rgba(0, 0, 0, 0.85);
    color: #e0e0e0;
    padding: 12px 16px;
    border-radius: 8px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    border: 1px solid rgba(226, 185, 59, 0.3);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.6);
    min-width: 240px;
    user-select: none;
  }

  .panel-title {
    color: #e2b93b;
    font-weight: bold;
    font-size: 13px;
    margin-bottom: 10px;
    letter-spacing: 1px;
  }

  .sub-tools {
    display: flex;
    gap: 2px;
    margin-bottom: 8px;
  }

  .sub-tool-btn {
    flex: 1;
    padding: 4px 8px;
    font-size: 10px;
    font-family: inherit;
    font-weight: bold;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.05);
    color: #888;
    cursor: pointer;
    letter-spacing: 0.5px;
  }

  .sub-tool-btn:hover {
    color: #ccc;
    background: rgba(255, 255, 255, 0.1);
  }

  .sub-tool-btn.active {
    background: rgba(226, 185, 59, 0.2);
    color: #e2b93b;
    border-color: rgba(226, 185, 59, 0.4);
  }

  .section-label {
    color: #888;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-bottom: 4px;
    margin-top: 8px;
  }

  .furniture-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 150px;
    overflow-y: auto;
  }

  .furniture-item-btn {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 8px;
    font-size: 11px;
    color: #999;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 3px;
    cursor: pointer;
    font-family: inherit;
    text-align: left;
    width: 100%;
  }

  .furniture-item-btn:hover {
    color: #ccc;
    background: rgba(255, 255, 255, 0.1);
  }

  .furniture-item-btn.active {
    background: rgba(68, 204, 255, 0.15);
    border-color: rgba(68, 204, 255, 0.4);
    color: #44ccff;
  }

  .item-name {
    font-weight: bold;
  }

  .item-action {
    font-size: 9px;
    color: #666;
  }

  .rotation-display {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    background: rgba(68, 204, 255, 0.1);
    border: 1px solid rgba(68, 204, 255, 0.2);
    border-radius: 3px;
  }

  .rotation-value {
    color: #44ccff;
    font-weight: bold;
    font-size: 13px;
  }

  .rotation-hint {
    color: #666;
    font-size: 9px;
  }

  .selected-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .coord-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 3px;
    font-size: 10px;
    background: rgba(68, 204, 255, 0.1);
    border: 1px solid rgba(68, 204, 255, 0.2);
  }

  .info-label {
    color: #888;
    width: 30px;
    flex-shrink: 0;
  }

  .info-value {
    color: #ccc;
    flex: 1;
  }

  .delete-btn {
    margin-top: 4px;
    width: 100%;
    padding: 5px;
    background: rgba(255, 60, 60, 0.2);
    border: 1px solid rgba(255, 60, 60, 0.4);
    border-radius: 4px;
    color: #ff6666;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    font-weight: bold;
  }

  .delete-btn:hover {
    background: rgba(255, 60, 60, 0.35);
  }

  .placement-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 100px;
    overflow-y: auto;
  }

  .placement-row {
    display: flex;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 3px;
    font-size: 10px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .placement-row.active {
    background: rgba(68, 204, 255, 0.15);
    border-color: rgba(68, 204, 255, 0.4);
  }

  .placement-type {
    color: #e2b93b;
    font-weight: bold;
    width: 50px;
    flex-shrink: 0;
  }

  .placement-pos {
    color: #888;
    flex: 1;
  }

  .draw-hint {
    padding: 6px 8px;
    background: rgba(226, 185, 59, 0.1);
    border: 1px solid rgba(226, 185, 59, 0.2);
    border-radius: 4px;
    color: #ccc;
    font-size: 10px;
    text-align: center;
    margin-top: 8px;
  }
</style>
