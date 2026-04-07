<script module lang="ts">
  import { SvelteMap } from 'svelte/reactivity'

  const REGION_SIZE = 16
  const TILE_DIM = 64
  const REGION_PX = REGION_SIZE * TILE_DIM // 1024

  const MIN_ZOOM = 1
  const MAX_ZOOM = 32
  const DEFAULT_ZOOM = 8

  // --- Image cache (module-level, persists across component lifecycle) ---
  const imageCache = new SvelteMap<string, HTMLImageElement | null>()
  const pendingLoads = new SvelteMap<string, Promise<HTMLImageElement | null>>()

  // --- Persisted view state (survives dialog close/reopen) ---
  let savedCamX: number | null = null
  let savedCamZ: number | null = null
  let savedZoom: number | null = null
</script>

<script lang="ts">
  import { gameStore } from '../stores/gameStore'
  import { worldMapVisible, debugVisible, teleportLoading } from '../stores/debugStore'
  import { showGenerateDialog, editorHeightManager, editorSplatManager, editorMetaManager, editorGrassDataManager, editorTreeDataManager, regionMetaVersion, minimapVersion, terrainForceRebuild } from '../stores/editorStore'
  import { get } from 'svelte/store'
  import { regionMinimapServerUrl, generateRegionMinimap, fetchHousesInRegion } from '../terrain/regionMinimapGenerator'
  import { tileToRegion } from '../managers/terrainMetaManager'
  import { getTerrainApiUrl } from '../utils/networkUtils'
  import { networkManager } from '../network/socket'
  import { regenerateRegionSplatmaps, type TerrainGenConfig } from '../terrain/terrainGenerator'
  import { generateAndSaveGrassData, removeGrassInRect, removeGrassNearTrees, encodeGrassBuffer, type GrassPlacementData } from '../utils/grass-data'
  import { generateAndSaveTreeData } from '../utils/tree-data'
  import { worldToTileCoord, tileKey } from '../managers/terrain-height-types'
  import type { TerrainGrassDataManager } from '../managers/terrainGrassDataManager'

  function loadRegionImage(rx: number, rz: number): Promise<HTMLImageElement | null> {
    const key = `${rx},${rz}`
    if (imageCache.has(key)) return Promise.resolve(imageCache.get(key)!)
    if (pendingLoads.has(key)) return pendingLoads.get(key)!

    const promise = new Promise<HTMLImageElement | null>((resolve) => {
      const img = new Image()
      img.onload = () => {
        imageCache.set(key, img)
        pendingLoads.delete(key)
        resolve(img)
      }
      img.onerror = () => {
        imageCache.set(key, null)
        pendingLoads.delete(key)
        resolve(null)
      }
      img.src = regionMinimapServerUrl(rx, rz)
    })
    pendingLoads.set(key, promise)
    return promise
  }

  // --- Component state ---
  let containerEl = $state<HTMLDivElement>()
  let canvasEl = $state<HTMLCanvasElement>()
  let containerW = $state(0)
  let containerH = $state(0)

  let playerX = $derived($gameStore.currentPlayer?.position.x ?? 0)
  let playerZ = $derived($gameStore.currentPlayer?.position.z ?? 0)

  let playerRegionRx = $derived(tileToRegion(Math.round(playerX / TILE_DIM)))
  let playerRegionRz = $derived(tileToRegion(Math.round(playerZ / TILE_DIM)))
  let deleting = $state(false)
  let resplatting = $state(false)
  let resplatProgress = $state('')
  let generatingMinimap = $state(false)

  // --- Camera state (world coordinates of view center) ---
  let camX = $state(0)
  let camZ = $state(0)

  // --- Zoom state (in regions/km) ---
  let zoomSpan = $state(DEFAULT_ZOOM)

  // Restore saved view state or center on player when dialog opens
  $effect(() => {
    if ($worldMapVisible) {
      if (savedCamX !== null && savedCamZ !== null) {
        camX = savedCamX
        camZ = savedCamZ
      } else {
        camX = playerX
        camZ = playerZ
      }
      if (savedZoom !== null) {
        zoomSpan = savedZoom
      }
    }
  })

  // --- Drag state ---
  let isDragging = $state(false)
  let dragStartMouseX = 0
  let dragStartMouseZ = 0
  let dragStartCamX = 0
  let dragStartCamZ = 0

  // --- Minimap version tracking: flush cache when minimaps are regenerated ---
  $effect(() => {
    const _ver = $minimapVersion // track dependency
    imageCache.clear()
    pendingLoads.clear()
  })

  // --- Canvas rendering ---
  let renderGeneration = 0

  $effect(() => {
    if (!canvasEl || containerW <= 0 || containerH <= 0) return

    const _mmVer = $minimapVersion // re-render when minimaps change
    const span = zoomSpan
    const cx = camX
    const cz = camZ
    const px = playerX
    const pz = playerZ
    const cw = containerW
    const ch = containerH
    const gen = ++renderGeneration

    const ctx = canvasEl.getContext('2d')!

    // Scale: how many canvas pixels per world unit
    // At current zoom, we show `span` regions across the shorter dimension
    const viewSize = span * REGION_PX // world units visible along shorter axis
    const canvasSize = Math.min(cw, ch)
    const scale = canvasSize / viewSize

    // World-space extents of the viewport
    const viewWorldW = cw / scale
    const viewWorldH = ch / scale

    // World-space top-left of viewport
    const viewLeft = cx - viewWorldW / 2
    const viewTop = cz - viewWorldH / 2

    // Clear to black
    ctx.clearRect(0, 0, cw, ch)
    ctx.fillStyle = '#000'
    ctx.fillRect(0, 0, cw, ch)

    // 45-degree rotation: expand visible region to cover rotated corners
    const ROTATE_ANGLE = -Math.PI / 4
    const expand = Math.SQRT2 // rotated square needs ~1.41x coverage

    const expandedViewWorldW = viewWorldW * expand
    const expandedViewWorldH = viewWorldH * expand
    const expandedViewLeft = cx - expandedViewWorldW / 2
    const expandedViewTop = cz - expandedViewWorldH / 2

    const expRegionMinRx = Math.floor((expandedViewLeft + TILE_DIM / 2) / REGION_PX)
    const expRegionMaxRx = Math.floor((expandedViewLeft + expandedViewWorldW + TILE_DIM / 2) / REGION_PX)
    const expRegionMinRz = Math.floor((expandedViewTop + TILE_DIM / 2) / REGION_PX)
    const expRegionMaxRz = Math.floor((expandedViewTop + expandedViewWorldH + TILE_DIM / 2) / REGION_PX)

    const promises: Promise<void>[] = []
    for (let rz = expRegionMinRz; rz <= expRegionMaxRz; rz++) {
      for (let rx = expRegionMinRx; rx <= expRegionMaxRx; rx++) {
        // Region world origin
        const regionWorldX = rx * REGION_PX - TILE_DIM / 2
        const regionWorldZ = rz * REGION_PX - TILE_DIM / 2

        // Canvas position (before rotation, relative to view center)
        const drawX = Math.floor((regionWorldX - viewLeft) * scale)
        const drawY = Math.floor((regionWorldZ - viewTop) * scale)
        const drawSize = Math.ceil(REGION_PX * scale)

        promises.push(
          loadRegionImage(rx, rz).then((img) => {
            if (gen !== renderGeneration) return
            if (img) {
              ctx.save()
              ctx.translate(cw / 2, ch / 2)
              ctx.rotate(ROTATE_ANGLE)
              ctx.translate(-cw / 2, -ch / 2)
              ctx.drawImage(img, drawX, drawY, drawSize, drawSize)
              ctx.restore()
            }
          })
        )
      }
    }

    Promise.all(promises).then(() => {
      if (gen !== renderGeneration) return

      // Player marker (also rotated with the map)
      const playerCanvasX = (px - viewLeft) * scale
      const playerCanvasZ = (pz - viewTop) * scale

      ctx.save()
      ctx.translate(cw / 2, ch / 2)
      ctx.rotate(ROTATE_ANGLE)
      ctx.translate(-cw / 2, -ch / 2)
      ctx.beginPath()
      ctx.arc(playerCanvasX, playerCanvasZ, 6, 0, Math.PI * 2)
      ctx.fillStyle = '#ff3333'
      ctx.fill()
      ctx.lineWidth = 2
      ctx.strokeStyle = '#ffffff'
      ctx.stroke()
      ctx.shadowColor = 'rgba(255, 50, 50, 0.8)'
      ctx.shadowBlur = 6
      ctx.beginPath()
      ctx.arc(playerCanvasX, playerCanvasZ, 6, 0, Math.PI * 2)
      ctx.fillStyle = '#ff3333'
      ctx.fill()
      ctx.restore()
    })
  })

  // --- Zoom controls ---
  function zoomIn() {
    zoomSpan = Math.max(MIN_ZOOM, zoomSpan - 1)
  }

  function zoomOut() {
    zoomSpan = Math.min(MAX_ZOOM, zoomSpan + 1)
  }

  function zoomReset() {
    zoomSpan = DEFAULT_ZOOM
    savedZoom = null
  }

  function resetCamera() {
    camX = playerX
    camZ = playerZ
    savedCamX = null
    savedCamZ = null
  }

  function handleWheel(event: WheelEvent) {
    event.preventDefault()
    if (event.deltaY > 0) {
      zoomOut()
    } else {
      zoomIn()
    }
  }

  $effect(() => {
    if (!containerEl) return
    containerEl.addEventListener('wheel', handleWheel, { passive: false })
    return () => containerEl!.removeEventListener('wheel', handleWheel)
  })

  // --- Drag to pan ---
  function handleMouseDown(event: MouseEvent) {
    if (event.ctrlKey) return // let Ctrl+click through for teleport
    if (event.button !== 0) return
    isDragging = true
    dragStartMouseX = event.clientX
    dragStartMouseZ = event.clientY
    dragStartCamX = camX
    dragStartCamZ = camZ
  }

  function handleMouseMove(event: MouseEvent) {
    if (!isDragging) return
    const viewSize = zoomSpan * REGION_PX
    const canvasSize = Math.min(containerW, containerH)
    const scale = canvasSize / viewSize

    // Rotate mouse delta by +45 degrees to undo the canvas rotation
    const dx = (event.clientX - dragStartMouseX) / scale
    const dz = (event.clientY - dragStartMouseZ) / scale
    const angle = Math.PI / 4
    const cosA = Math.cos(angle)
    const sinA = Math.sin(angle)
    camX = dragStartCamX - (dx * cosA - dz * sinA)
    camZ = dragStartCamZ - (dx * sinA + dz * cosA)
  }

  function handleMouseUp() {
    isDragging = false
  }

  $effect(() => {
    if (!isDragging) return
    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mouseup', handleMouseUp)
    }
  })

  // Save view state on component destroy (covers all close paths)
  $effect(() => {
    return () => {
      savedCamX = camX
      savedCamZ = camZ
      savedZoom = zoomSpan
    }
  })

  // --- Debug actions ---
  function handleGenerate() {
    showGenerateDialog.set({ rx: playerRegionRx, rz: playerRegionRz })
    close()
  }

  async function handleDelete() {
    const rx = playerRegionRx
    const rz = playerRegionRz
    if (!confirm(`Delete all terrain data for region (${rx}, ${rz})?`)) return

    deleting = true
    try {
      const resp = await fetch(
        `${getTerrainApiUrl()}/api/terrain/region/${rx}/${rz}`,
        { method: 'DELETE' }
      )
      if (!resp.ok) throw new Error(`Server returned ${resp.status}`)

      // Evict cached tile data (without disposing GPU resources still in use)
      const heightManager = get(editorHeightManager)
      const splatManager = get(editorSplatManager)
      const metaManager = get(editorMetaManager)
      for (let tz = 0; tz < REGION_SIZE; tz++) {
        for (let tx = 0; tx < REGION_SIZE; tx++) {
          const tileX = rx * REGION_SIZE + tx
          const tileZ = rz * REGION_SIZE + tz
          heightManager?.evictCachedData(tileX, tileZ)
          splatManager?.evictCachedData(tileX, tileZ)
        }
      }
      metaManager?.invalidateRegion(rx, rz)
      regionMetaVersion.update((v) => v + 1)

      // Invalidate minimap cache
      imageCache.delete(`${rx},${rz}`)
      pendingLoads.delete(`${rx},${rz}`)
      minimapVersion.update((v) => v + 1)

      // Force terrain tiles to rebuild with fresh server data
      terrainForceRebuild.update((v) => v + 1)

      close()
    } catch (e) {
      console.error('Failed to delete region:', e)
      alert(`Failed to delete region: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      deleting = false
    }
  }

  async function handleRegenerateMinimap() {
    const rx = playerRegionRx
    const rz = playerRegionRz
    const metaManager = get(editorMetaManager)
    if (!metaManager) return

    generatingMinimap = true
    try {
      await generateRegionMinimap(rx, rz, metaManager)
      imageCache.delete(`${rx},${rz}`)
      pendingLoads.delete(`${rx},${rz}`)
      minimapVersion.update((v) => v + 1)
    } catch (e) {
      console.error('Minimap generation failed:', e)
      alert(`Minimap generation failed: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      generatingMinimap = false
    }
  }

  /** Fetch all houses in a region and remove grass under their ground-floor rooms.
   *  Returns the set of tile keys that have houses on them. */
  async function removeGrassUnderHousesInRegion(
    rx: number,
    rz: number,
    grassMgr: TerrainGrassDataManager
  ): Promise<Set<string>> {
    const GRASS_MARGIN = 1
    const apiUrl = getTerrainApiUrl()
    const houses = await fetchHousesInRegion(rx, rz, apiUrl)

    // Collect removal rects per tile so each tile is saved at most once
    const tileRects = new SvelteMap<string, { tx: number; tz: number; rects: [number, number, number, number][] }>()
    for (const house of houses) {
      for (const room of house.rooms) {
        if (room.floorLevel !== 0 || room.roomType === 'stairwell') continue
        const minX = house.origin.x + room.localX - GRASS_MARGIN
        const minZ = house.origin.z + room.localZ - GRASS_MARGIN
        const maxX = house.origin.x + room.localX + room.sizeX + GRASS_MARGIN
        const maxZ = house.origin.z + room.localZ + room.sizeZ + GRASS_MARGIN

        for (let tz = worldToTileCoord(minZ); tz <= worldToTileCoord(maxZ); tz++) {
          for (let tx = worldToTileCoord(minX); tx <= worldToTileCoord(maxX); tx++) {
            const key = tileKey(tx, tz)
            let entry = tileRects.get(key)
            if (!entry) {
              entry = { tx, tz, rects: [] }
              tileRects.set(key, entry)
            }
            entry.rects.push([minX, minZ, maxX, maxZ])
          }
        }
      }
    }

    // Apply all rects per tile, then batch-save
    const saves: Promise<void>[] = []
    for (const { tx, tz, rects } of tileRects.values()) {
      let data = grassMgr.getCachedGrassData(tx, tz)
      if (!data) continue
      for (const [minX, minZ, maxX, maxZ] of rects) {
        const filtered = removeGrassInRect(data, minX, minZ, maxX, maxZ)
        if (filtered) data = filtered
      }
      saves.push(grassMgr.saveGrassData(tx, tz, data))
      if (saves.length >= 8) {
        await Promise.all(saves.splice(0))
      }
    }
    if (saves.length > 0) await Promise.all(saves)
    return new Set(tileRects.keys())
  }

  async function handleResplat() {
    const rx = playerRegionRx
    const rz = playerRegionRz
    const heightManager = get(editorHeightManager)
    const splatManager = get(editorSplatManager)
    const metaManager = get(editorMetaManager)
    if (!heightManager || !splatManager || !metaManager) return

    resplatting = true
    resplatProgress = 'Loading heightmaps...'
    try {
      const apiUrl = getTerrainApiUrl()

      // Check if region exists
      const metaResp = await fetch(`${apiUrl}/api/terrain/meta/${rx}/${rz}`, { method: 'HEAD' })
      if (!metaResp.ok) {
        alert(`Region (${rx}, ${rz}) has no terrain data.`)
        return
      }

      // Fetch all heightmaps for the region
      const tileHeightmaps: { tileX: number; tileZ: number; heightmap: Uint16Array }[] = []
      const fetches: Promise<void>[] = []
      for (let tz = 0; tz < REGION_SIZE; tz++) {
        for (let tx = 0; tx < REGION_SIZE; tx++) {
          const tileX = rx * REGION_SIZE + tx
          const tileZ = rz * REGION_SIZE + tz
          fetches.push(
            heightManager.loadHeightmap(tileX, tileZ).then((data) => {
              tileHeightmaps.push({ tileX, tileZ, heightmap: data })
            })
          )
        }
      }
      await Promise.all(fetches)

      const config: TerrainGenConfig = {
        seed: 0, // only used for grass noise seed
        minHeight: -20,
        maxHeight: 80,
        seaProportion: 0,
        plainProportion: 0,
        mountainProportion: 0,
        shallowSeaRatio: 0,
        riverCount: 0,
      }

      // Estimate maxHeight from actual heightmap data
      let actualMax = 0
      for (const tile of tileHeightmaps) {
        for (let i = 0; i < tile.heightmap.length; i++) {
          const h = tile.heightmap[i] * 0.05 - 500.0
          if (h > actualMax) actualMax = h
        }
      }
      config.maxHeight = Math.max(actualMax, 1)

      // Regenerate splatmaps
      resplatProgress = 'Generating splatmaps...'
      await new Promise((r) => setTimeout(r, 0))
      const results = regenerateRegionSplatmaps(rx, rz, tileHeightmaps, config)

      // Apply to managers
      for (const tile of results) {
        splatManager.setSplatmap(tile.tileX, tile.tileZ, tile.splatmap)
        splatManager.markDirty(tile.tileX, tile.tileZ)
      }

      // Save splatmaps to server in batches
      resplatProgress = 'Saving splatmaps...'
      const BATCH_SIZE = 8
      for (let i = 0; i < results.length; i += BATCH_SIZE) {
        const batch = results.slice(i, i + BATCH_SIZE)
        await Promise.all(
          batch.map((tile) =>
            fetch(`${apiUrl}/api/terrain/splat/${tile.tileX}/${tile.tileZ}`, {
              method: 'PUT',
              headers: { 'Content-Type': 'application/octet-stream' },
              body: tile.splatmap.slice().buffer as ArrayBuffer,
            })
          )
        )
      }

      // Generate and save grass placement data, then suppress under houses
      const grassMgr = get(editorGrassDataManager)
      try {
      if (grassMgr) {
        grassMgr.suppressListeners = true
        await generateAndSaveGrassData(results, heightManager, grassMgr, (label) => {
          resplatProgress = label
        })

        resplatProgress = 'Removing grass under houses...'
        const houseTiles = await removeGrassUnderHousesInRegion(rx, rz, grassMgr)

        // Update grass-original for tiles WITHOUT houses so demolition
        // restores post-resplat grass. Skip tiles with houses — their
        // original snapshot must keep pre-flatten Y values.
        resplatProgress = 'Updating grass snapshots...'
        const SAVE_BATCH = 8
        const tilesToUpdate = results.filter(
          (t) => !houseTiles.has(tileKey(t.tileX, t.tileZ))
        )
        for (let i = 0; i < tilesToUpdate.length; i += SAVE_BATCH) {
          const batch = tilesToUpdate.slice(i, i + SAVE_BATCH)
          await Promise.all(
            batch.map((tile) => {
              const cached = grassMgr.getCachedGrassData(tile.tileX, tile.tileZ)
              if (!cached) return
              const buf = encodeGrassBuffer(cached, tile.tileX, tile.tileZ)
              return fetch(`${apiUrl}/api/terrain/grass-original/${tile.tileX}/${tile.tileZ}`, {
                method: 'PUT',
                headers: { 'Content-Type': 'application/octet-stream' },
                body: buf,
              }).catch(() => {})
            })
          )
        }
      }

      // Generate and save tree placement data, then remove grass overlapping tree trunks
      const treeMgr = get(editorTreeDataManager)
      if (treeMgr) {
        await generateAndSaveTreeData(results, heightManager, treeMgr, (label) => {
          resplatProgress = label
        })

        if (grassMgr) {
          resplatProgress = 'Removing grass near trees...'
          const dirtyTiles: { tileX: number; tileZ: number; data: GrassPlacementData }[] = []
          for (const tile of results) {
            const treeData = treeMgr.getCachedTreeData(tile.tileX, tile.tileZ)
            const grassData = grassMgr.getCachedGrassData(tile.tileX, tile.tileZ)
            if (!treeData || !grassData) continue
            const filtered = removeGrassNearTrees(grassData, treeData)
            if (!filtered) continue
            dirtyTiles.push({ tileX: tile.tileX, tileZ: tile.tileZ, data: filtered })
          }
          const TREE_SAVE_BATCH = 8
          for (let i = 0; i < dirtyTiles.length; i += TREE_SAVE_BATCH) {
            const batch = dirtyTiles.slice(i, i + TREE_SAVE_BATCH)
            await Promise.all(
              batch.map((t) => grassMgr.saveGrassData(t.tileX, t.tileZ, t.data))
            )
          }
        }
      }
      } finally {
        if (grassMgr) grassMgr.suppressListeners = false
      }

      // Regenerate minimap
      resplatProgress = 'Generating minimap...'
      await generateRegionMinimap(rx, rz, metaManager)
      minimapVersion.update((v) => v + 1)

      // Force terrain rebuild
      terrainForceRebuild.update((v) => v + 1)

      close()
    } catch (e) {
      console.error('Resplat failed:', e)
      alert(`Resplat failed: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      resplatting = false
    }
  }

  // --- Actions ---
  function close() {
    worldMapVisible.set(false)
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      close()
    }
  }

  function handleBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) {
      close()
    }
  }

  function handleMapClick(event: MouseEvent) {
    if (!event.ctrlKey || !containerEl || containerW <= 0 || containerH <= 0) return
    event.preventDefault()
    event.stopPropagation()

    const rect = containerEl.getBoundingClientRect()
    const pixelX = event.clientX - rect.left
    const pixelY = event.clientY - rect.top

    const viewSize = zoomSpan * REGION_PX
    const canvasSize = Math.min(containerW, containerH)
    const scale = canvasSize / viewSize

    // Screen offset from center, then rotate by +45 degrees to undo canvas rotation
    const sx = (pixelX - containerW / 2) / scale
    const sz = (pixelY - containerH / 2) / scale
    const angle = Math.PI / 4
    const cosA = Math.cos(angle)
    const sinA = Math.sin(angle)
    const worldX = camX + (sx * cosA - sz * sinA)
    const worldZ = camZ + (sx * sinA + sz * cosA)

    const position = { x: worldX, y: 0, z: worldZ }

    gameStore.update((state) => {
      if (!state.currentPlayer) return state
      state.currentPlayer.position.set(worldX, 0, worldZ)
      return state
    })

    networkManager.sendDebugTeleport(position)
    teleportLoading.set(true)
    close()
  }

  // --- Resize observer ---
  $effect(() => {
    if (!containerEl) return
    const ro = new ResizeObserver((entries) => {
      const entry = entries[0]
      if (entry) {
        containerW = entry.contentRect.width
        containerH = entry.contentRect.height
      }
    })
    ro.observe(containerEl)
    return () => ro.disconnect()
  })
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={handleBackdropClick}>
  <div class="dialog" role="dialog" aria-modal="true">
    <div class="header">
      <h2>World Map ({zoomSpan}&times;{zoomSpan} km)</h2>
      <div class="controls">
        <button class="ctrl-btn" onclick={zoomIn} title="Zoom In">+</button>
        <button class="ctrl-btn" onclick={zoomOut} title="Zoom Out">&minus;</button>
        <button class="ctrl-btn" onclick={zoomReset} title="Reset Zoom">1:{DEFAULT_ZOOM}</button>
        <button class="ctrl-btn" onclick={resetCamera} title="Center on Player">&#8982;</button>
        {#if $debugVisible}
          <span class="controls-separator"></span>
          <button class="ctrl-btn debug-btn" onclick={handleGenerate} title="Generate terrain for region ({playerRegionRx}, {playerRegionRz})">Gen</button>
          <button class="ctrl-btn debug-btn" onclick={handleResplat} disabled={resplatting} title="Regenerate splatmaps for region ({playerRegionRx}, {playerRegionRz})">{resplatting ? resplatProgress : 'Resplat'}</button>
          <button class="ctrl-btn debug-btn" onclick={handleRegenerateMinimap} disabled={generatingMinimap} title="Regenerate minimap for region ({playerRegionRx}, {playerRegionRz})">{generatingMinimap ? 'Minimap...' : 'Minimap'}</button>
          <button class="ctrl-btn debug-btn danger" onclick={handleDelete} disabled={deleting} title="Delete terrain for region ({playerRegionRx}, {playerRegionRz})">Del</button>
        {/if}
      </div>
      <button class="close-btn" onclick={close}>&times;</button>
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="map-container"
      class:dragging={isDragging}
      bind:this={containerEl}
      onmousedown={handleMouseDown}
      onclick={handleMapClick}
    >
      <canvas
        bind:this={canvasEl}
        width={containerW}
        height={containerH}
        class="map-canvas"
      ></canvas>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
    z-index: 30;
  }

  .dialog {
    width: min(80vw, 800px);
    height: min(80vh, 800px);
    display: flex;
    flex-direction: column;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(16, 16, 16, 0.95);
    color: #f4f4f4;
    overflow: hidden;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }

  .header h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }

  .controls {
    display: flex;
    gap: 4px;
  }

  .ctrl-btn {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: #ccc;
    font-size: 14px;
    cursor: pointer;
    padding: 2px 8px;
    line-height: 1.4;
  }

  .ctrl-btn:hover {
    background: rgba(255, 255, 255, 0.2);
    color: #fff;
  }

  .controls-separator {
    width: 1px;
    height: 18px;
    background: rgba(255, 255, 255, 0.15);
    margin: 0 4px;
  }

  .debug-btn {
    color: #e2b93b;
    border-color: rgba(226, 185, 59, 0.3);
  }

  .debug-btn:hover {
    background: rgba(226, 185, 59, 0.2);
    color: #f0c94d;
  }

  .debug-btn.danger {
    color: #ff6b6b;
    border-color: rgba(255, 107, 107, 0.3);
  }

  .debug-btn.danger:hover {
    background: rgba(255, 107, 107, 0.2);
    color: #ff8888;
  }

  .debug-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .close-btn {
    background: none;
    border: none;
    color: #aaa;
    font-size: 22px;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
  }

  .close-btn:hover {
    color: #fff;
  }

  .map-container {
    flex: 1;
    position: relative;
    min-height: 0;
    overflow: hidden;
    cursor: grab;
  }

  .map-container.dragging {
    cursor: grabbing;
  }

  .map-canvas {
    position: absolute;
    inset: 0;
    display: block;
  }

</style>
