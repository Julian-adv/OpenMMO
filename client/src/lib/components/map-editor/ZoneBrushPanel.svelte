<script lang="ts">
  import {
    zoneSubTool,
    zoneDrawStart,
    currentEditorRegion,
    editorZoneManager,
    hoveredZoneIndex,
    currentZoneData,
    spawnFormMonsterType,
    spawnFormMaxPerPlayer,
    spawnFormMaxTotal,
    spawnFormIntervalSecs,
    noSpawnFormLabel,
  } from '../../stores/editorStore'
  import type { ZoneSubTool } from '../../stores/editorStore'
  import type { ZoneData, NoSpawnZone, MonsterSpawnZone } from '../../managers/zoneManager'
  import { get } from 'svelte/store'

  let subTool = $state<ZoneSubTool>('noSpawn')
  let region = $state<{ rx: number; rz: number } | null>(null)
  let zoneData = $state<ZoneData>({ monsterSpawns: [], noSpawnZones: [] })
  let drawStart = $state<{ x: number; z: number } | null>(null)

  let spawnMonsterType = $state('scp939')
  let spawnMaxPerPlayer = $state(3)
  let spawnMaxTotal = $state(10)
  let spawnIntervalSecs = $state(30)
  let noSpawnLabel = $state('')

  zoneSubTool.subscribe((v) => (subTool = v))
  zoneDrawStart.subscribe((v) => (drawStart = v))
  currentZoneData.subscribe((v) => (zoneData = v))
  spawnFormMonsterType.subscribe((v) => (spawnMonsterType = v))
  spawnFormMaxPerPlayer.subscribe((v) => (spawnMaxPerPlayer = v))
  spawnFormMaxTotal.subscribe((v) => (spawnMaxTotal = v))
  spawnFormIntervalSecs.subscribe((v) => (spawnIntervalSecs = v))
  noSpawnFormLabel.subscribe((v) => (noSpawnLabel = v))

  currentEditorRegion.subscribe(async (v) => {
    region = v
    if (v) {
      await loadZoneData(v.rx, v.rz)
    }
  })

  async function loadZoneData(rx: number, rz: number) {
    const mgr = get(editorZoneManager)
    if (!mgr) return
    const data = await mgr.fetchZone(rx, rz)
    currentZoneData.set(data)
  }

  function selectSubTool(tool: ZoneSubTool) {
    zoneSubTool.set(tool)
    zoneDrawStart.set(null)
  }

  async function saveAndUpdate(updated: ZoneData) {
    if (!region) return
    const mgr = get(editorZoneManager)
    if (!mgr) return
    await mgr.saveZone(region.rx, region.rz, updated)
    currentZoneData.set(updated)
  }

  async function deleteZone(key: 'noSpawnZones' | 'monsterSpawns', index: number) {
    const arr = [...(zoneData[key] ?? [])]
    arr.splice(index, 1)
    await saveAndUpdate({ ...zoneData, [key]: arr })
  }

  /** Called from MapEditorCursor when the user finishes drawing a rectangle. */
  export async function addNoSpawnZone(minX: number, minZ: number, maxX: number, maxZ: number) {
    const zone: NoSpawnZone = { minX, minZ, maxX, maxZ }
    if (noSpawnLabel.trim()) zone.label = noSpawnLabel.trim()
    const zones = [...(zoneData.noSpawnZones ?? []), zone]
    await saveAndUpdate({ ...zoneData, noSpawnZones: zones })
    noSpawnFormLabel.set('')
  }

  export async function addSpawnZone(minX: number, minZ: number, maxX: number, maxZ: number) {
    const zone: MonsterSpawnZone = {
      monsterType: spawnMonsterType,
      maxPerPlayer: spawnMaxPerPlayer,
      maxTotal: spawnMaxTotal,
      spawnIntervalSecs: spawnIntervalSecs,
      minX, minZ, maxX, maxZ,
    }
    const spawns = [...(zoneData.monsterSpawns ?? []), zone]
    await saveAndUpdate({ ...zoneData, monsterSpawns: spawns })
  }

  function formatCoord(n: number): string {
    return n.toFixed(0)
  }
</script>

<div class="zone-panel">
  <div class="panel-title">Zone Editor</div>

  <div class="sub-tool-tabs">
    <button
      class="sub-tab"
      class:active={subTool === 'noSpawn'}
      onclick={() => selectSubTool('noSpawn')}
    >No-Spawn</button>
    <button
      class="sub-tab"
      class:active={subTool === 'spawn'}
      onclick={() => selectSubTool('spawn')}
    >Spawn</button>
  </div>

  {#if subTool === 'noSpawn'}
    <div class="section-label">No-Spawn Zones</div>
    <div class="zone-list">
      {#each zoneData.noSpawnZones ?? [] as zone, i (i)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="zone-item no-spawn"
          onmouseenter={() => hoveredZoneIndex.set({ type: 'noSpawn', index: i })}
          onmouseleave={() => hoveredZoneIndex.set(null)}
        >
          <span class="zone-info">
            {zone.label || 'Zone'} ({formatCoord(zone.minX)},{formatCoord(zone.minZ)}) - ({formatCoord(zone.maxX)},{formatCoord(zone.maxZ)})
          </span>
          <button class="delete-btn" onclick={() => deleteZone('noSpawnZones', i)}>x</button>
        </div>
      {/each}
      {#if (zoneData.noSpawnZones ?? []).length === 0}
        <div class="empty-msg">No zones defined</div>
      {/if}
    </div>

    <div class="section-label">New Zone</div>
    <div class="control-row">
      <label for="zone-label">Label</label>
      <input id="zone-label" type="text" value={noSpawnLabel} oninput={(e) => noSpawnFormLabel.set((e.target as HTMLInputElement).value)} placeholder="e.g. Town" />
    </div>
    <div class="draw-hint">
      {#if drawStart}
        Click second corner to finish ({formatCoord(drawStart.x)}, {formatCoord(drawStart.z)})
      {:else}
        Click on map to set first corner
      {/if}
    </div>
  {:else}
    <div class="section-label">Spawn Zones</div>
    <div class="zone-list">
      {#each zoneData.monsterSpawns ?? [] as zone, i (i)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="zone-item spawn"
          onmouseenter={() => hoveredZoneIndex.set({ type: 'spawn', index: i })}
          onmouseleave={() => hoveredZoneIndex.set(null)}
        >
          <span class="zone-info">
            {zone.monsterType} ({formatCoord(zone.minX)},{formatCoord(zone.minZ)}) - ({formatCoord(zone.maxX)},{formatCoord(zone.maxZ)})
          </span>
          <button class="delete-btn" onclick={() => deleteZone('monsterSpawns', i)}>x</button>
        </div>
      {/each}
      {#if (zoneData.monsterSpawns ?? []).length === 0}
        <div class="empty-msg">No spawn zones defined</div>
      {/if}
    </div>

    <div class="section-label">New Spawn Zone</div>
    <div class="control-row">
      <label for="spawn-type">Monster</label>
      <input id="spawn-type" type="text" value={spawnMonsterType} oninput={(e) => spawnFormMonsterType.set((e.target as HTMLInputElement).value)} />
    </div>
    <div class="control-row">
      <label for="spawn-max-player">Per Player</label>
      <input id="spawn-max-player" type="number" min="1" max="50" value={spawnMaxPerPlayer} oninput={(e) => spawnFormMaxPerPlayer.set(parseInt((e.target as HTMLInputElement).value) || 1)} />
    </div>
    <div class="control-row">
      <label for="spawn-max-total">Max Total</label>
      <input id="spawn-max-total" type="number" min="1" max="200" value={spawnMaxTotal} oninput={(e) => spawnFormMaxTotal.set(parseInt((e.target as HTMLInputElement).value) || 1)} />
    </div>
    <div class="control-row">
      <label for="spawn-interval">Interval(s)</label>
      <input id="spawn-interval" type="number" min="1" max="600" value={spawnIntervalSecs} oninput={(e) => spawnFormIntervalSecs.set(parseInt((e.target as HTMLInputElement).value) || 1)} />
    </div>
    <div class="draw-hint">
      {#if drawStart}
        Click second corner to finish ({formatCoord(drawStart.x)}, {formatCoord(drawStart.z)})
      {:else}
        Click on map to set first corner
      {/if}
    </div>
  {/if}
</div>

<style>
  .zone-panel {
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

  .sub-tool-tabs {
    display: flex;
    gap: 2px;
    margin-bottom: 8px;
  }

  .sub-tab {
    flex: 1;
    padding: 4px 8px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    color: #888;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    font-weight: bold;
    transition: background 150ms ease, color 150ms ease;
  }

  .sub-tab:hover {
    color: #ccc;
  }

  .sub-tab.active {
    background: rgba(226, 185, 59, 0.25);
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

  .zone-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 120px;
    overflow-y: auto;
  }

  .zone-item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px;
    border-radius: 3px;
    font-size: 10px;
  }

  .zone-item.no-spawn {
    background: rgba(255, 80, 80, 0.15);
    border: 1px solid rgba(255, 80, 80, 0.3);
  }

  .zone-item.spawn {
    background: rgba(80, 180, 255, 0.15);
    border: 1px solid rgba(80, 180, 255, 0.3);
  }

  .zone-info {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .delete-btn {
    background: rgba(255, 60, 60, 0.3);
    border: none;
    color: #ff6666;
    cursor: pointer;
    border-radius: 3px;
    padding: 1px 5px;
    font-family: inherit;
    font-size: 10px;
    font-weight: bold;
  }

  .delete-btn:hover {
    background: rgba(255, 60, 60, 0.5);
  }

  .empty-msg {
    color: #555;
    font-size: 10px;
    font-style: italic;
    padding: 4px 0;
  }

  .control-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  .control-row label {
    width: 70px;
    flex-shrink: 0;
    color: #aaa;
    font-size: 11px;
  }

  .control-row input[type='text'],
  .control-row input[type='number'] {
    flex: 1;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 3px;
    color: #e0e0e0;
    padding: 3px 6px;
    font-family: inherit;
    font-size: 11px;
    min-width: 0;
  }

  .control-row input:focus {
    outline: none;
    border-color: rgba(226, 185, 59, 0.5);
  }

  .draw-hint {
    margin-top: 8px;
    padding: 6px 8px;
    background: rgba(226, 185, 59, 0.1);
    border: 1px solid rgba(226, 185, 59, 0.2);
    border-radius: 4px;
    color: #ccc;
    font-size: 10px;
    text-align: center;
  }
</style>
