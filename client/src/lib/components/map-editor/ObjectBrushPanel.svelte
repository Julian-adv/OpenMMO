<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import {
    objectCatalog,
    selectedObjectType,
    objectRotation,
    currentObjectData,
    selectedObjectPlacementId,
    objectSubTool,
    editorHeightManager,
    editorGrassDataManager,
  } from '../../stores/editorStore'
  import type {
    ObjectDef,
    ObjectPlacement,
    ObjectRegionData,
    ObjectSubTool,
  } from '../../stores/editorStore'
  import { objectManager } from '../../managers/objectManager'
  import { rotatedRectAabb, type FootprintRect } from '../../utils/objectFootprint'
  import { removeGrassInRect } from '../../utils/grass-data'
  import { worldToTileCoord, tileKey } from '../../managers/terrain-height-types'
  import { playerFloorLevel } from '../../stores/housingStore'
  import { currentEditorRegion } from '../../stores/editorStore'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainGrassDataManager } from '../../managers/terrainGrassDataManager'

  let catalog = $state<ObjectDef[]>([])
  let selected = $state<string | null>(null)
  let rotation = $state(0)
  let placements = $state<ObjectPlacement[]>([])
  let selectedPlacementId = $state<number | null>(null)
  let subTool = $state<ObjectSubTool>('place')
  let floor = $state(-1)
  /** Anchors slider range so it doesn't drift while dragging. */
  let baseY = $state<number | null>(null)
  let heightManager = $state<TerrainHeightManager | null>(null)
  let grassManager = $state<TerrainGrassDataManager | null>(null)
  let flattening = $state(false)

  const Y_RANGE = 5
  /** Tight blend so flattening doesn't bleed into surrounding terrain (e.g. river banks). */
  const FLATTEN_BLEND_RADIUS = 2

  const unsubs = [
    objectCatalog.subscribe((v) => (catalog = v)),
    selectedObjectType.subscribe((v) => (selected = v)),
    objectRotation.subscribe((v) => (rotation = v)),
    currentObjectData.subscribe((v) => (placements = v.placements)),
    selectedObjectPlacementId.subscribe((id) => {
      selectedPlacementId = id
      if (id === null) {
        baseY = null
      } else {
        const p = get(currentObjectData).placements.find((p) => p.id === id)
        baseY = p?.y ?? null
      }
    }),
    objectSubTool.subscribe((v) => (subTool = v)),
    playerFloorLevel.subscribe((v) => (floor = v)),
    editorHeightManager.subscribe((v) => (heightManager = v)),
    editorGrassDataManager.subscribe((v) => (grassManager = v)),
  ]
  onDestroy(() => unsubs.forEach((u) => u()))

  onMount(async () => {
    if (get(objectCatalog).length > 0) return
    const list = await objectManager.fetchCatalog()
    objectCatalog.set(list)
  })

  function selectType(id: string) {
    selectedObjectType.set(id)
    objectSubTool.set('place')
    selectedObjectPlacementId.set(null)
  }

  function setSubTool(tool: ObjectSubTool) {
    objectSubTool.set(tool)
    if (tool === 'place') {
      selectedObjectPlacementId.set(null)
    }
  }

  function applyY(newY: number): ObjectRegionData | null {
    if (selectedPlacementId === null) return null
    const data = get(currentObjectData)
    const updated: ObjectRegionData = {
      placements: data.placements.map((p) =>
        p.id === selectedPlacementId ? { ...p, y: newY } : p
      ),
    }
    currentObjectData.set(updated)
    return updated
  }

  function previewY(newY: number) {
    applyY(newY)
  }

  function commitY(newY: number) {
    const updated = applyY(newY)
    if (!updated) return
    const region = get(currentEditorRegion)
    if (region) {
      objectManager.saveObject(region.rx, region.rz, updated)
    }
  }

  async function flattenTerrain() {
    const p = selectedPlacement
    const hm = heightManager
    if (!p || !hm || flattening) return

    flattening = true
    try {
      const fp = await objectManager.fetchFootprint(p.type)
      if (!fp || fp.rects.length === 0) return

      const buryDepth = objectManager.getCatalogEntry(p.type)?.flattenBuryDepth ?? 0
      const targetY = p.y + fp.minLocalY + buryDepth
      // Flatten using the local rect + placement rotation so footprints rotated
      // off-axis (e.g. 45°) carve the right oriented region instead of bleeding
      // into the rotated rect's AABB corners.
      for (const r of fp.rects) {
        hm.flattenRotatedRect(
          p.x,
          p.z,
          p.rotation,
          r.minX,
          r.maxX,
          r.minZ,
          r.maxZ,
          targetY,
          FLATTEN_BLEND_RADIUS
        )
      }
      await hm.saveAllDirty()

      // World AABB of each rotated rect, used for grass-removal tile bucketing.
      // (removeGrassInRect is AABB-only — at non-90° rotations this over-clears
      // grass slightly into the AABB corners, which is acceptable.)
      const rot = (p.rotation * Math.PI) / 180
      const worldRects: FootprintRect[] = fp.rects.map((r) => {
        const a = rotatedRectAabb(r.minX, r.maxX, r.minZ, r.maxZ, rot)
        return {
          minX: p.x + a.minX,
          maxX: p.x + a.maxX,
          minZ: p.z + a.minZ,
          maxZ: p.z + a.maxZ,
        }
      })

      const gm = grassManager
      if (!gm) return

      // eslint-disable-next-line svelte/prefer-svelte-reactivity
      const tileBuckets = new Map<string, { tx: number; tz: number; rects: FootprintRect[] }>()
      for (const wr of worldRects) {
        const txMin = worldToTileCoord(wr.minX)
        const txMax = worldToTileCoord(wr.maxX)
        const tzMin = worldToTileCoord(wr.minZ)
        const tzMax = worldToTileCoord(wr.maxZ)
        for (let tx = txMin; tx <= txMax; tx++) {
          for (let tz = tzMin; tz <= tzMax; tz++) {
            const key = tileKey(tx, tz)
            let bucket = tileBuckets.get(key)
            if (!bucket) {
              bucket = { tx, tz, rects: [] }
              tileBuckets.set(key, bucket)
            }
            bucket.rects.push(wr)
          }
        }
      }

      await Promise.all(
        [...tileBuckets.values()].map(async ({ tx, tz, rects }) => {
          let data = gm.getCachedGrassData(tx, tz) ?? (await gm.loadGrassData(tx, tz))
          if (!data) return
          let changed = false
          for (const r of rects) {
            const next = removeGrassInRect(data, r.minX, r.minZ, r.maxX, r.maxZ)
            if (next) {
              data = next
              changed = true
            }
          }
          if (changed) await gm.saveGrassData(tx, tz, data)
        })
      )
    } finally {
      flattening = false
    }
  }

  async function deletePlacement() {
    if (selectedPlacementId === null) return
    const data = get(currentObjectData)
    const updated: ObjectRegionData = {
      placements: data.placements.filter((p) => p.id !== selectedPlacementId),
    }
    currentObjectData.set(updated)
    selectedObjectPlacementId.set(null)

    const region = get(currentEditorRegion)
    if (region) {
      await objectManager.saveObject(region.rx, region.rz, updated)
    }
  }

  function formatPos(p: ObjectPlacement): string {
    return `${p.x.toFixed(1)}, ${p.y.toFixed(1)}, ${p.z.toFixed(1)}`
  }

  let selectedPlacement = $derived(
    placements.find((p) => p.id === selectedPlacementId) ?? null
  )
</script>

<div class="object-panel">
  <div class="panel-title">Object</div>

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
    <div class="object-list">
      {#each catalog as item (item.id)}
        <button
          class="object-item-btn"
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
      <div class="draw-hint">Select an object type to place</div>
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
        {#if baseY !== null}
          <div class="coord-row">
            <span class="info-label">Y:</span>
            <input
              class="y-slider"
              type="range"
              min={baseY - Y_RANGE}
              max={baseY + Y_RANGE}
              step="0.05"
              value={selectedPlacement.y}
              oninput={(e) => previewY(parseFloat(e.currentTarget.value))}
              onchange={(e) => commitY(parseFloat(e.currentTarget.value))}
            />
            <span class="y-value">{selectedPlacement.y.toFixed(2)}</span>
          </div>
        {/if}
        <div class="coord-row">
          <span class="info-label">Rot:</span>
          <span class="info-value">{selectedPlacement.rotation}&deg;</span>
        </div>
        <div class="coord-row">
          <span class="info-label">Floor:</span>
          <span class="info-value">{selectedPlacement.floorLevel + 1}F</span>
        </div>
        <button
          class="flatten-btn"
          onclick={flattenTerrain}
          disabled={flattening || !heightManager}
        >
          {flattening ? 'Flattening…' : 'Flatten Terrain'}
        </button>
        <button class="delete-btn" onclick={deletePlacement}>Delete</button>
      </div>
    {:else}
      <div class="draw-hint">Click a placed object to select</div>
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
  .object-panel {
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

  .object-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 150px;
    overflow-y: auto;
  }

  .object-item-btn {
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

  .object-item-btn:hover {
    color: #ccc;
    background: rgba(255, 255, 255, 0.1);
  }

  .object-item-btn.active {
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

  .y-slider {
    flex: 1;
    min-width: 0;
    height: 14px;
    accent-color: #44ccff;
    margin: 0;
  }

  .y-value {
    color: #44ccff;
    font-variant-numeric: tabular-nums;
    width: 42px;
    text-align: right;
    flex-shrink: 0;
  }

  .flatten-btn {
    margin-top: 4px;
    width: 100%;
    padding: 5px;
    background: rgba(68, 204, 255, 0.15);
    border: 1px solid rgba(68, 204, 255, 0.4);
    border-radius: 4px;
    color: #44ccff;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    font-weight: bold;
  }

  .flatten-btn:hover:not(:disabled) {
    background: rgba(68, 204, 255, 0.3);
  }

  .flatten-btn:disabled {
    opacity: 0.5;
    cursor: default;
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
