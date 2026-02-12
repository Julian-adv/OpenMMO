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
  import { cameraDistance } from '../stores/cameraStore'
  import { timeScale } from '../stores/timeStore'
  import {
    debugVisible,
    cameraRotationEnabled,
    playerDebugInfo,
  } from '../stores/debugStore'

  function toDegrees(radians: number) {
    const degrees = (radians * 180) / Math.PI
    return ((degrees % 360) + 360) % 360
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.ctrlKey && event.key === 'd') {
      event.preventDefault()
      debugVisible.update((v) => !v)
    }
  }

  function toggleSlowMode() {
    timeScale.update((scale) => (scale === 1.0 ? 0.1 : 1.0))
  }

  function toggleCameraRotation() {
    cameraRotationEnabled.update((v) => !v)
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="hud-container">
  <div class="hud-box">
    {#if $debugVisible}
      <div class="stats-text">
        <span class="fps-text">
          FPS: {currentFps} | ZOOM: {$cameraDistance.toFixed(1)}
        </span>
        {#if $playerDebugInfo}
          <span class="player-text">
            POS: ({$playerDebugInfo.position.x.toFixed(2)},
            {$playerDebugInfo.position.y.toFixed(2)},
            {$playerDebugInfo.position.z.toFixed(2)}) | ROT:
            {toDegrees($playerDebugInfo.rotation).toFixed(1)}°
          </span>
        {:else}
          <span class="player-text">POS: (-, -, -) | ROT: -</span>
        {/if}
      </div>
    {/if}

    <div class="button-group">
      <button
        class="action-btn slow-btn"
        class:active={$timeScale < 1.0}
        onclick={toggleSlowMode}
        title="Toggle Slow Motion"
      >
        SLOW
      </button>
      
      <button
        class="action-btn"
        class:active={$cameraRotationEnabled}
        onclick={toggleCameraRotation}
        title="Toggle Camera Rotation"
      >
        CAM ROT: {$cameraRotationEnabled ? 'ON' : 'OFF'}
      </button>
    </div>
  </div>
</div>

<style>
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
    align-items: center;
    gap: 15px;
    width: fit-content;
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
</style>
