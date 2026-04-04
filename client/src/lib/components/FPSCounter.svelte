<script lang="ts" module>
  let frameCount = 0
  let lastFpsTime = 0
  let currentFps = $state(0)

  export function initFpsCounting() {
    lastFpsTime = performance.now()
  }

  export function tickFps(currentTime: number) {
    frameCount++
    if (currentTime - lastFpsTime >= 1000) {
      currentFps = Math.round((frameCount * 1000) / (currentTime - lastFpsTime))
      frameCount = 0
      lastFpsTime = currentTime
    }
  }
</script>

<script lang="ts">
  import { currentBgmTrack } from '../managers/bgmManager'
  import { networkManager } from '../network/socket'
  import { cameraDistance } from '../stores/cameraStore'
  import { worldToTileCell } from './game-scene/terrain-utils'
  import { tileToRegion } from '../managers/terrainMetaManager'
  import { timeScale, sunTimeScale } from '../stores/timeStore'
  import {
    debugVisible,
    cameraRotationEnabled,
    calendarVisible,
    celestialDebugVisible,
    playerDebugInfo,
    mapEditorMode,
    housingEditorMode,
    gridVisible,
    worldMapVisible,
    inventoryVisible,
    characterPanelVisible,
    debugSpeedMode,
    refractionEnabled,
    reflectionEnabled,
    torchLightEnabled,
    windDebugVisible,
  } from '../stores/debugStore'

  function toDegrees(radians: number) {
    const degrees = (radians * 180) / Math.PI
    return ((degrees % 360) + 360) % 360
  }


  function isGameKey(event: KeyboardEvent): boolean {
    if (event.ctrlKey || event.altKey || event.metaKey) return false
    const tag = (document.activeElement?.tagName ?? '').toLowerCase()
    return tag !== 'input' && tag !== 'textarea'
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.ctrlKey && event.key === 'd') {
      event.preventDefault()
      debugVisible.update((v) => !v)
    }
    if (event.ctrlKey && event.key === 'm') {
      event.preventDefault()
      mapEditorMode.update((v) => !v)
    }
    if ((event.key === 'm' || event.key === 'M') && isGameKey(event)) {
      event.preventDefault()
      worldMapVisible.update((v) => !v)
    }
    if ((event.key === 'i' || event.key === 'I') && isGameKey(event)) {
      event.preventDefault()
      inventoryVisible.update((v) => !v)
    }
    if ((event.key === 'c' || event.key === 'C') && isGameKey(event)) {
      event.preventDefault()
      characterPanelVisible.update((v) => !v)
    }
  }

  function toggleSlowMode() {
    timeScale.update((scale) => (scale === 1.0 ? 0.1 : 1.0))
  }

  function setSunSpeed(scale: number) {
    sunTimeScale.set(scale)
  }

  function toggleCameraRotation() {
    cameraRotationEnabled.update((v) => !v)
  }

  function toggleCalendar() {
    calendarVisible.update((v) => !v)
  }

  function toggleCelestialDebug() {
    celestialDebugVisible.update((v: boolean) => !v)
  }

  function toggleGrid() {
    gridVisible.update((v) => !v)
  }

  function toggleDebugSpeed() {
    debugSpeedMode.update((v) => !v)
  }

  function toggleRefraction() {
    refractionEnabled.update((v) => !v)
  }

  function toggleReflection() {
    reflectionEnabled.update((v) => !v)
  }

  function toggleMapEditor() {
    mapEditorMode.update((v) => !v)
  }

  function toggleTorchLight() {
    torchLightEnabled.update((v) => {
      const newValue = !v
      networkManager.sendTorchToggle(newValue)
      return newValue
    })
  }

  function toggleWindDebug() {
    windDebugVisible.update((v) => !v)
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if !$debugVisible}
  <button class="debug-toggle-btn" onclick={() => debugVisible.set(true)} title="Show Debug Panel (Ctrl+D)">
    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 2a4 4 0 0 0-4 4v2H6a2 2 0 0 0-2 2v1h4" /><path d="M18 8h-2V6a4 4 0 0 0-4-4" /><path d="M20 10a2 2 0 0 0-2-2" /><path d="M2 13h4" /><path d="M18 13h4" /><path d="M6 18H4a2 2 0 0 1-2-2" /><path d="M20 18h2" /><path d="M6 8v10a6 6 0 0 0 12 0V8" /><path d="M2 10h4" /><path d="M18 10h4" />
    </svg>
  </button>
{:else}
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="hud-container" role="button" tabindex="-1" onclick={() => debugVisible.set(false)}>
  <div class="hud-box">
      <div class="stats-text">
        <span class="fps-text">
          FPS: {currentFps} | ZOOM: {$cameraDistance.toFixed(1)}
        </span>
        {#if $currentBgmTrack}
          <span class="bgm-text">♫ {$currentBgmTrack}</span>
        {/if}
        {#if $playerDebugInfo}
          {@const tc = worldToTileCell($playerDebugInfo.position.x, $playerDebugInfo.position.z)}
          <span class="player-text">
            POS: ({$playerDebugInfo.position.x.toFixed(2)},
            {$playerDebugInfo.position.y.toFixed(2)},
            {$playerDebugInfo.position.z.toFixed(2)}) | ROT:
            {toDegrees($playerDebugInfo.rotation).toFixed(1)}°
          </span>
          <span class="player-text">
            RGN: ({tileToRegion(tc.tileX)}, {tileToRegion(tc.tileZ)}) | TILE: ({tc.tileX}, {tc.tileZ}) | CELL: ({tc.cellX}, {tc.cellZ})
          </span>
        {:else}
          <span class="player-text">POS: (-, -, -) | ROT: -</span>
        {/if}
      </div>

    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="button-rows" onclick={(e) => e.stopPropagation()}>
      <div class="button-group">
        <button
          class="action-btn slow-btn"
          class:active={$timeScale < 1.0}
          onclick={toggleSlowMode}
          title="Toggle Slow Motion"
        >
          SLOW TIME
        </button>

        <div class="seg-group" title="Sun Speed">
          <span class="seg-label">FAST DAY</span>
          <button
            class="seg-btn"
            class:seg-active={$sunTimeScale === 1.0}
            onclick={() => setSunSpeed(1.0)}
          >OFF</button>
          <button
            class="seg-btn"
            class:seg-active={$sunTimeScale === 60.0}
            onclick={() => setSunSpeed(60.0)}
          >3m</button>
          <button
            class="seg-btn"
            class:seg-active={$sunTimeScale === 600.0}
            onclick={() => setSunSpeed(600.0)}
          >18s</button>
        </div>

        <button
          class="action-btn"
          class:active={$cameraRotationEnabled}
          onclick={toggleCameraRotation}
          title="Toggle Camera Rotation"
        >
          CAM ROT
        </button>

        <button
          class="action-btn cal-btn"
          class:active={$calendarVisible}
          onclick={toggleCalendar}
          title="Toggle Calendar Display"
        >
          CAL
        </button>
      </div>

      <div class="button-group">
        <button
          class="action-btn orbits-btn"
          class:active={$celestialDebugVisible}
          onclick={toggleCelestialDebug}
          title="Toggle Celestial Orbits Debug"
        >
          ORBITS
        </button>

        {#if !$mapEditorMode}
          <button
            class="action-btn grid-btn"
            class:active={$gridVisible}
            onclick={toggleGrid}
            title="Toggle Terrain Grid"
          >
            GRID
          </button>
        {/if}

        <button
          class="action-btn map-editor-btn"
          class:active={$mapEditorMode}
          onclick={toggleMapEditor}
          title="Toggle Map Editor (Ctrl+M)"
        >
          MAP EDIT
        </button>

        <button
          class="action-btn"
          class:active={$housingEditorMode}
          onclick={() => housingEditorMode.update((v) => !v)}
          title="Toggle Housing Editor"
        >
          HOUSE
        </button>

        <button
          class="action-btn debug-speed-btn"
          class:active={$debugSpeedMode}
          onclick={toggleDebugSpeed}
          title="Debug Mode: 10x Speed + Extended Zoom"
        >
          FAST MOVE
        </button>

      </div>

      <div class="button-group">
        <button
          class="action-btn refraction-btn"
          class:active={$refractionEnabled}
          onclick={toggleRefraction}
          title="Toggle Water Refraction"
        >
          REFRACT
        </button>

        <button
          class="action-btn reflection-btn"
          class:active={$reflectionEnabled}
          onclick={toggleReflection}
          title="Toggle Water Reflection"
        >
          REFLECT
        </button>

        <button
          class="action-btn torch-btn"
          class:active={$torchLightEnabled}
          onclick={toggleTorchLight}
          title="Toggle Torch Point Light"
        >
          TORCH
        </button>

        <button
          class="action-btn wind-btn"
          class:active={$windDebugVisible}
          onclick={toggleWindDebug}
          title="Toggle Wind Direction Arrow"
        >
          WIND
        </button>
      </div>
    </div>
  </div>
</div>
{/if}

<style>
  .debug-toggle-btn {
    position: fixed;
    top: 10px;
    left: 10px;
    z-index: 1000;
    background: rgba(0, 0, 0, 0.6);
    color: rgba(255, 255, 255, 0.5);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    padding: 6px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .debug-toggle-btn:hover {
    background: rgba(0, 0, 0, 0.8);
    color: #00ff00;
    border-color: rgba(0, 255, 0, 0.3);
  }

  .hud-container {
    position: fixed;
    top: 10px;
    left: 10px;
    z-index: 1000;
    pointer-events: none;
  }

  .hud-box {
    background: rgba(0, 0, 0, 0.8);
    color: #00ff00;
    padding: 8px 12px;
    border-radius: 6px;
    font-family: 'Courier New', monospace;
    font-size: 14px;
    font-weight: bold;
    pointer-events: auto;
    border: 1px solid rgba(0, 255, 0, 0.3);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: flex-start;
    gap: 15px;
    width: fit-content;
    cursor: pointer;
  }

  .fps-text {
    white-space: nowrap;
  }

  .stats-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    align-items: flex-start;
    text-align: left;
  }

  .player-text {
    white-space: nowrap;
  }

  .bgm-text {
    color: #e2b93b;
    white-space: nowrap;
  }

  .button-rows {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .button-group {
    display: flex;
    gap: 6px;
  }

  .action-btn {
    background: #333;
    color: #fff;
    border: 1px solid #666;
    border-radius: 4px;
    padding: 4px 8px;
    font-size: 11px;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.2s;
    white-space: nowrap;
  }

  .action-btn:hover {
    background: #555;
  }

  .action-btn.active {
    background: #2f855a; /* Green for CAM ROT ON */
    border-color: #68d391;
  }

  .action-btn.slow-btn.active {
    background: #c53030; /* Red for Slow Mode */
    border-color: #feb2b2;
  }

  .seg-group {
    display: flex;
    align-items: center;
    gap: 0;
    border: 1px solid #666;
    border-radius: 4px;
    overflow: hidden;
  }

  .seg-label {
    padding: 4px 6px;
    font-size: 11px;
    color: #aaa;
    background: #222;
    border-right: 1px solid #666;
    white-space: nowrap;
  }

  .seg-btn {
    background: #333;
    color: #fff;
    border: none;
    border-right: 1px solid #555;
    padding: 4px 8px;
    font-size: 11px;
    cursor: pointer;
    font-family: inherit;
    font-weight: bold;
    transition: background 0.15s;
    white-space: nowrap;
  }

  .seg-btn:last-child {
    border-right: none;
  }

  .seg-btn:hover {
    background: #555;
  }

  .seg-btn.seg-active {
    background: #b7791f;
    color: #fff;
  }

  .action-btn.cal-btn.active {
    background: #2b6cb0;
    border-color: #63b3ed;
  }

  .action-btn.orbits-btn.active {
    background: #553b8a;
    border-color: #b794f4;
  }

  .action-btn.grid-btn.active {
    background: #b7791f;
    border-color: #ecc94b;
  }

  .action-btn.map-editor-btn.active {
    background: #2c7a7b;
    border-color: #4fd1c5;
  }

  .action-btn.debug-speed-btn.active {
    background: #c05621;
    border-color: #ed8936;
  }

  .action-btn.refraction-btn.active {
    background: #2b6cb0;
    border-color: #63b3ed;
  }

  .action-btn.reflection-btn.active {
    background: #553b8a;
    border-color: #b794f4;
  }

  .action-btn.torch-btn.active {
    background: #b7791f;
    border-color: #ecc94b;
  }

  .action-btn.wind-btn.active {
    background: #2f855a;
    border-color: #68d391;
  }
</style>
