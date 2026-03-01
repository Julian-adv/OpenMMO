<script lang="ts">
  import { gameStore } from '../stores/gameStore'
  import { worldMapVisible, teleportLoading } from '../stores/debugStore'
  import { networkManager } from '../network/socket'

  const MAP_WIDTH = 3696
  const MAP_HEIGHT = 3924

  let containerEl = $state<HTMLDivElement>()
  let containerW = $state(0)
  let containerH = $state(0)

  let playerX = $derived($gameStore.currentPlayer?.position.x ?? 0)
  let playerZ = $derived($gameStore.currentPlayer?.position.z ?? 0)

  // Fit image into container preserving aspect ratio
  let scale = $derived(Math.min(containerW / MAP_WIDTH, containerH / MAP_HEIGHT))
  let drawW = $derived(MAP_WIDTH * scale)
  let drawH = $derived(MAP_HEIGHT * scale)
  let offsetX = $derived((containerW - drawW) / 2)
  let offsetY = $derived((containerH - drawH) / 2)

  // Player marker position: center of image = world (0,0), 1px = 1m
  let markerLeft = $derived(offsetX + (MAP_WIDTH / 2 + playerX) * scale)
  let markerTop = $derived(offsetY + (MAP_HEIGHT / 2 + playerZ) * scale)

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
    if (!event.ctrlKey || !containerEl || scale <= 0) return
    event.preventDefault()
    event.stopPropagation()

    const rect = containerEl.getBoundingClientRect()
    const pixelX = event.clientX - rect.left
    const pixelY = event.clientY - rect.top

    // Reverse the marker calculation: markerLeft = offsetX + (MAP_WIDTH/2 + playerX) * scale
    const worldX = (pixelX - offsetX) / scale - MAP_WIDTH / 2
    const worldZ = (pixelY - offsetY) / scale - MAP_HEIGHT / 2

    const position = { x: worldX, y: 0, z: worldZ }

    // Optimistic local update
    gameStore.update((state) => {
      if (!state.currentPlayer) return state
      state.currentPlayer.position.set(worldX, 0, worldZ)
      return state
    })

    networkManager.sendDebugTeleport(position)
    teleportLoading.set(true)
    close()
  }

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
      <h2>World Map</h2>
      <button class="close-btn" onclick={close}>&times;</button>
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="map-container" bind:this={containerEl} onclick={handleMapClick}>
      {#if scale > 0}
        <img
          src="/textures/height_map.png"
          alt="World Map"
          class="map-image"
          style="width: {drawW}px; height: {drawH}px; left: {offsetX}px; top: {offsetY}px;"
        />
        <div
          class="player-marker"
          style="left: {markerLeft}px; top: {markerTop}px;"
        ></div>
      {/if}
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

  .map-image {
    position: absolute;
    display: block;
  }

  .player-marker {
    position: absolute;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ff3333;
    border: 2px solid #ffffff;
    transform: translate(-50%, -50%);
    pointer-events: none;
    box-shadow: 0 0 6px rgba(255, 50, 50, 0.8);
  }
</style>
