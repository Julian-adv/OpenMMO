<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity'
  import { gameStore } from '../stores/gameStore'
  import { worldMapVisible, teleportLoading } from '../stores/debugStore'
  import { regionMinimapServerUrl } from '../terrain/regionMinimapGenerator'
  import { networkManager } from '../network/socket'

  const REGION_SIZE = 16
  const TILE_DIM = 64
  const REGION_PX = REGION_SIZE * TILE_DIM // 1024

  const ZOOM_LEVELS = [1, 4, 8, 16, 32]
  const DEFAULT_ZOOM_INDEX = 2

  // --- Image cache (module-level, persists across re-renders) ---
  const imageCache = new SvelteMap<string, HTMLImageElement | null>()
  const pendingLoads = new SvelteMap<string, Promise<HTMLImageElement | null>>()

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

  // --- Zoom state (normal mode) ---
  let zoomIndex = $state(DEFAULT_ZOOM_INDEX)
  let zoomSpan = $derived(ZOOM_LEVELS[zoomIndex])

  // Player's region coordinates
  let playerRx = $derived(Math.floor((playerX + TILE_DIM / 2) / REGION_PX))
  let playerRz = $derived(Math.floor((playerZ + TILE_DIM / 2) / REGION_PX))

  // Visible region range
  let halfSpan = $derived(Math.floor(zoomSpan / 2))
  let startRx = $derived(playerRx - halfSpan)
  let startRz = $derived(playerRz - halfSpan)

  // --- Canvas rendering ---
  let renderGeneration = 0

  $effect(() => {
    if (!canvasEl || containerW <= 0 || containerH <= 0) return

    const span = zoomSpan
    const srx = startRx
    const srz = startRz
    const px = playerX
    const pz = playerZ
    const cw = containerW
    const ch = containerH
    const gen = ++renderGeneration

    const ctx = canvasEl.getContext('2d')!
    const regionDrawSize = Math.min(cw, ch) / span
    const gridW = span * regionDrawSize
    const gridH = span * regionDrawSize
    const ox = (cw - gridW) / 2
    const oy = (ch - gridH) / 2

    // Clear to black
    ctx.clearRect(0, 0, cw, ch)
    ctx.fillStyle = '#000'
    ctx.fillRect(0, 0, cw, ch)

    const promises: Promise<void>[] = []
    for (let dz = 0; dz < span; dz++) {
      for (let dx = 0; dx < span; dx++) {
        const rx = srx + dx
        const rz = srz + dz
        const drawX = ox + dx * regionDrawSize
        const drawY = oy + dz * regionDrawSize

        promises.push(
          loadRegionImage(rx, rz).then((img) => {
            if (gen !== renderGeneration) return
            if (img) {
              ctx.drawImage(img, drawX, drawY, regionDrawSize, regionDrawSize)
            }
          })
        )
      }
    }

    Promise.all(promises).then(() => {
      if (gen !== renderGeneration) return

      // Player marker
      const playerCanvasX = ox + ((px + TILE_DIM / 2) / REGION_PX - srx) * regionDrawSize
      const playerCanvasZ = oy + ((pz + TILE_DIM / 2) / REGION_PX - srz) * regionDrawSize

      ctx.save()
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

  // --- Mouse wheel zoom ---
  function handleWheel(event: WheelEvent) {
    event.preventDefault()
    if (event.deltaY > 0) {
      zoomIndex = Math.min(ZOOM_LEVELS.length - 1, zoomIndex + 1)
    } else {
      zoomIndex = Math.max(0, zoomIndex - 1)
    }
  }

  $effect(() => {
    if (!containerEl) return
    containerEl.addEventListener('wheel', handleWheel, { passive: false })
    return () => containerEl!.removeEventListener('wheel', handleWheel)
  })

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

    const span = zoomSpan
    const regionDrawSize = Math.min(containerW, containerH) / span
    const gridW = span * regionDrawSize
    const gridH = span * regionDrawSize
    const ox = (containerW - gridW) / 2
    const oy = (containerH - gridH) / 2

    const worldX = ((pixelX - ox) / regionDrawSize + startRx) * REGION_PX - TILE_DIM / 2
    const worldZ = ((pixelY - oy) / regionDrawSize + startRz) * REGION_PX - TILE_DIM / 2

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
      <button class="close-btn" onclick={close}>&times;</button>
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="map-container" bind:this={containerEl} onclick={handleMapClick}>
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
  }

  .map-canvas {
    position: absolute;
    inset: 0;
    display: block;
  }

</style>
