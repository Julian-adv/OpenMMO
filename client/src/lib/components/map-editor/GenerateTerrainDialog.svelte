<script lang="ts">
  import { get } from 'svelte/store'
  import {
    showGenerateDialog,
    currentEditorRegion,
    editorMetaManager,
    editorHeightManager,
    editorSplatManager,
    regionMetaVersion,
  } from '../../stores/editorStore'
  import {
    generateRegionTerrain,
    type TerrainGenConfig,
    type GeneratedTile,
    type NeighborEdgeData,
  } from '../../terrain/terrainGenerator'
  import type { RegionMeta } from '../../managers/terrainMetaManager'
  import { getTerrainApiUrl } from '../../utils/networkUtils'

  const REGION_SIZE = 16
  const TILE_DIM = 64

  let seed = $state(Math.floor(Math.random() * 100000))
  let minHeight = $state(-20)
  let maxHeight = $state(80)
  let seaPct = $state(30)
  let shallowSeaPct = $state(30)
  let plainPct = $state(50)
  let mountainPct = $state(20)
  let riverCount = $state(2)

  let generating = $state(false)
  let progress = $state(0)
  let progressLabel = $state('')

  function randomizeSeed() {
    seed = Math.floor(Math.random() * 100000)
  }

  function close() {
    showGenerateDialog.set(false)
  }

  async function fetchNeighborEdges(
    rx: number,
    rz: number
  ): Promise<NeighborEdgeData> {
    const apiUrl = getTerrainApiUrl()
    const edges: NeighborEdgeData = {}

    const directions = [
      { key: 'north' as const, drx: 0, drz: -1, row: TILE_DIM - 1, isRow: true },
      { key: 'south' as const, drx: 0, drz: 1, row: 0, isRow: true },
      { key: 'west' as const, drx: -1, drz: 0, col: TILE_DIM - 1, isRow: false },
      { key: 'east' as const, drx: 1, drz: 0, col: 0, isRow: false },
    ]

    for (const dir of directions) {
      const nrx = rx + dir.drx
      const nrz = rz + dir.drz
      const edgeData = new Float32Array(REGION_SIZE * TILE_DIM)
      let hasData = false

      // Fetch boundary tiles from the neighbor region
      const fetches = []
      for (let t = 0; t < REGION_SIZE; t++) {
        const tileX = dir.isRow
          ? nrx * REGION_SIZE + t
          : nrx * REGION_SIZE + (dir.key === 'west' ? REGION_SIZE - 1 : 0)
        const tileZ = dir.isRow
          ? nrz * REGION_SIZE + (dir.key === 'north' ? REGION_SIZE - 1 : 0)
          : nrz * REGION_SIZE + t

        fetches.push(
          fetch(`${apiUrl}/api/terrain/height/${tileX}/${tileZ}`)
            .then(async (resp) => {
              if (!resp.ok) return null
              const buf = await resp.arrayBuffer()
              return { t, data: new Uint16Array(buf) }
            })
            .catch(() => null)
        )
      }

      const results = await Promise.all(fetches)
      for (const result of results) {
        if (!result) continue
        hasData = true
        const { t, data } = result

        for (let c = 0; c < TILE_DIM; c++) {
          const idx = dir.isRow
            ? (dir.key === 'north' ? TILE_DIM - 1 : 0) * TILE_DIM + c
            : c * TILE_DIM + (dir.key === 'west' ? TILE_DIM - 1 : 0)
          const encoded = data[idx]
          const meters = encoded * 0.05 - 500.0
          edgeData[t * TILE_DIM + c] = meters
        }
      }

      if (hasData) {
        edges[dir.key] = edgeData
      }
    }

    return edges
  }

  async function handleGenerate() {
    const region = get(currentEditorRegion)
    const heightManager = get(editorHeightManager)
    const splatManager = get(editorSplatManager)
    const metaManager = get(editorMetaManager)

    if (!region || !heightManager || !splatManager || !metaManager) return

    generating = true
    progress = 0
    progressLabel = 'Loading neighbor data...'

    try {
      // Fetch neighbor edges for boundary blending
      const neighborEdges = await fetchNeighborEdges(region.rx, region.rz)
      progress = 10
      progressLabel = 'Generating terrain...'

      // Allow UI to update
      await new Promise((r) => requestAnimationFrame(r))

      const config: TerrainGenConfig = {
        seed,
        minHeight,
        maxHeight,
        seaProportion: seaPct / 100,
        plainProportion: plainPct / 100,
        mountainProportion: mountainPct / 100,
        shallowSeaRatio: shallowSeaPct / 100,
        riverCount,
      }

      const tiles = generateRegionTerrain(
        region.rx,
        region.rz,
        config,
        neighborEdges
      )

      progress = 30
      progressLabel = 'Saving region meta...'

      // Set region meta for generation textures
      const genMeta: RegionMeta = {
        layers: [
          { texture: 'rocky_terrain_02_1k', tileScale: 8.0 },
          { texture: 'gravel_floor_1k', tileScale: 6.0 },
          { texture: 'sandy_gravel_02_1k', tileScale: 8.0 },
          { texture: 'snow_02_1k', tileScale: 4.0 },
        ],
      }
      await metaManager.saveMeta(region.rx, region.rz, genMeta)

      progress = 35
      progressLabel = 'Applying tiles...'

      // Inject data into managers
      for (const tile of tiles) {
        heightManager.setHeightmap(tile.tileX, tile.tileZ, tile.heightmap)
        splatManager.setSplatmap(tile.tileX, tile.tileZ, tile.splatmap)
        heightManager.markDirty(tile.tileX, tile.tileZ)
        splatManager.markDirty(tile.tileX, tile.tileZ)
      }

      // Update visible geometry
      for (const tile of tiles) {
        heightManager.refreshTileGeometry(tile.tileX, tile.tileZ)
        heightManager.refreshAdjacentTileEdges(tile.tileX, tile.tileZ)
      }

      progress = 50
      progressLabel = 'Saving to server...'

      // Save in parallel batches
      await saveTilesBatched(tiles)

      progress = 100
      progressLabel = 'Done!'

      // Trigger region meta re-resolution
      regionMetaVersion.update((v) => v + 1)

      // Brief delay to show completion
      await new Promise((r) => setTimeout(r, 300))
      close()
    } catch (e) {
      console.error('Terrain generation failed:', e)
      progressLabel = `Error: ${e instanceof Error ? e.message : String(e)}`
    } finally {
      generating = false
    }
  }

  async function saveTilesBatched(tiles: GeneratedTile[]) {
    const apiUrl = getTerrainApiUrl()
    const BATCH_SIZE = 8
    const totalBatches = Math.ceil(tiles.length / BATCH_SIZE)

    for (let i = 0; i < tiles.length; i += BATCH_SIZE) {
      const batch = tiles.slice(i, i + BATCH_SIZE)
      await Promise.all(
        batch.flatMap((tile) => [
          fetch(
            `${apiUrl}/api/terrain/height/${tile.tileX}/${tile.tileZ}`,
            {
              method: 'PUT',
              headers: { 'Content-Type': 'application/octet-stream' },
              body: tile.heightmap.slice().buffer as ArrayBuffer,
            }
          ),
          fetch(
            `${apiUrl}/api/terrain/splat/${tile.tileX}/${tile.tileZ}`,
            {
              method: 'PUT',
              headers: { 'Content-Type': 'application/octet-stream' },
              body: tile.splatmap.slice().buffer as ArrayBuffer,
            }
          ),
        ])
      )

      const batchNum = Math.floor(i / BATCH_SIZE) + 1
      progress = 50 + Math.round((batchNum / totalBatches) * 50)
      progressLabel = `Saving tiles... ${batchNum}/${totalBatches}`
    }
  }
</script>

<div class="backdrop" role="dialog" aria-modal="true">
  <div class="dialog">
    <h2>Generate Terrain</h2>

    {#if !generating}
      <div class="controls">
        <div class="control-row">
          <label for="terrain-seed">Seed</label>
          <div class="seed-row">
            <input
              id="terrain-seed"
              type="number"
              bind:value={seed}
              class="seed-input"
            />
            <button class="randomize-btn" onclick={randomizeSeed}>Random</button>
          </div>
        </div>

        <div class="control-row">
          <label for="terrain-min-height">Min Height <span class="value">{minHeight}m</span></label>
          <input id="terrain-min-height" type="range" min={-500} max={0} step={1} bind:value={minHeight} />
        </div>

        <div class="control-row">
          <label for="terrain-max-height">Max Height <span class="value">{maxHeight}m</span></label>
          <input id="terrain-max-height" type="range" min={1} max={3276} step={1} bind:value={maxHeight} />
        </div>

        <div class="separator"></div>

        <div class="control-row">
          <label for="terrain-sea">Sea <span class="value">{seaPct}%</span></label>
          <input id="terrain-sea" type="range" min={0} max={60} step={1} bind:value={seaPct} />
        </div>

        <div class="control-row sub-control">
          <label for="terrain-shallow-sea">Shallow Sea <span class="value">{shallowSeaPct}%</span></label>
          <input id="terrain-shallow-sea" type="range" min={0} max={80} step={1} bind:value={shallowSeaPct} />
        </div>

        <div class="control-row">
          <label for="terrain-plains">Plains <span class="value">{plainPct}%</span></label>
          <input id="terrain-plains" type="range" min={0} max={80} step={1} bind:value={plainPct} />
        </div>

        <div class="control-row">
          <label for="terrain-mountain">Mountain <span class="value">{mountainPct}%</span></label>
          <input id="terrain-mountain" type="range" min={0} max={60} step={1} bind:value={mountainPct} />
        </div>

        <div class="control-row">
          <label for="terrain-rivers">Rivers <span class="value">{riverCount}</span></label>
          <input id="terrain-rivers" type="range" min={0} max={5} step={1} bind:value={riverCount} />
        </div>
      </div>

      <div class="actions">
        <button class="primary" onclick={handleGenerate}>Generate</button>
        <button class="secondary" onclick={close}>Cancel</button>
      </div>
    {:else}
      <div class="progress-section">
        <div class="progress-bar">
          <div class="progress-fill" style="width: {progress}%"></div>
        </div>
        <p class="progress-label">{progressLabel}</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
    z-index: 2000;
  }

  .dialog {
    width: min(420px, calc(100vw - 32px));
    padding: 20px;
    border-radius: 12px;
    border: 1px solid rgba(226, 185, 59, 0.3);
    background: rgba(16, 16, 16, 0.95);
    color: #f4f4f4;
    font-family: 'Courier New', monospace;
  }

  h2 {
    margin: 0 0 16px 0;
    font-size: 16px;
    color: #e2b93b;
    letter-spacing: 1px;
  }

  .controls {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .control-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .control-row label {
    font-size: 11px;
    color: #aaa;
    display: flex;
    justify-content: space-between;
  }

  .value {
    color: #e2b93b;
    font-weight: bold;
  }

  .control-row input[type="range"] {
    width: 100%;
    accent-color: #e2b93b;
    height: 4px;
  }

  .seed-row {
    display: flex;
    gap: 6px;
  }

  .seed-input {
    flex: 1;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: #f4f4f4;
    padding: 4px 8px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
  }

  .randomize-btn {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: #ccc;
    padding: 4px 10px;
    cursor: pointer;
    font-family: 'Courier New', monospace;
    font-size: 11px;
  }

  .randomize-btn:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .sub-control {
    padding-left: 12px;
    border-left: 2px solid rgba(226, 185, 59, 0.2);
  }

  .separator {
    height: 1px;
    background: rgba(255, 255, 255, 0.1);
    margin: 4px 0;
  }

  .actions {
    display: flex;
    gap: 10px;
    justify-content: center;
    margin-top: 16px;
  }

  .actions button {
    border: none;
    border-radius: 8px;
    padding: 10px 20px;
    font-size: 13px;
    cursor: pointer;
    font-family: 'Courier New', monospace;
    font-weight: bold;
    letter-spacing: 0.5px;
  }

  .actions .primary {
    background: #e2b93b;
    color: #1a1a1a;
  }

  .actions .primary:hover {
    background: #f0c94d;
  }

  .actions .secondary {
    background: #3d3d3d;
    color: #f0f0f0;
  }

  .actions .secondary:hover {
    background: #4d4d4d;
  }

  .progress-section {
    margin-top: 12px;
  }

  .progress-bar {
    height: 6px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 3px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: #e2b93b;
    border-radius: 3px;
    transition: width 200ms ease;
  }

  .progress-label {
    margin: 8px 0 0 0;
    font-size: 11px;
    color: #aaa;
    text-align: center;
  }
</style>
