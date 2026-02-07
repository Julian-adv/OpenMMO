<script lang="ts">
  import { cameraDistance } from '../stores/cameraStore'
  import { timeScale } from '../stores/timeStore'

  let fps = $state(0)
  let frameCount = $state(0)
  let lastFpsTime = $state(0)
  let visible = $state(true)

  function updateFPS() {
    frameCount++
    const currentTime = performance.now()

    if (currentTime - lastFpsTime >= 1000) {
      // Update FPS every second
      fps = Math.round((frameCount * 1000) / (currentTime - lastFpsTime))
      frameCount = 0
      lastFpsTime = currentTime
    }

    requestAnimationFrame(updateFPS)
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.ctrlKey && event.key === 'd') {
      event.preventDefault()
      visible = !visible
    }
  }

  function toggleSlowMode() {
    timeScale.update((scale) => (scale === 1.0 ? 0.1 : 1.0))
  }

  // Start FPS monitoring
  lastFpsTime = performance.now()
  requestAnimationFrame(updateFPS)
</script>

<svelte:window onkeydown={handleKeydown} />

{#if visible}
  <div class="fps-counter">
    <span>FPS: {fps} | ZOOM: {$cameraDistance.toFixed(1)}</span>
    <button
      class="slow-btn"
      class:active={$timeScale < 1.0}
      onclick={toggleSlowMode}
    >
      SLOW
    </button>
  </div>
{/if}

<style>
  .fps-counter {
    position: fixed;
    top: 10px;
    left: 10px;
    background: rgba(0, 0, 0, 0.8);
    color: #00ff00;
    padding: 8px 12px;
    border-radius: 6px;
    font-family: 'Courier New', monospace;
    font-size: 14px;
    font-weight: bold;
    z-index: 1000;
    pointer-events: auto; /* Changed to auto to allow button click */
    border: 1px solid rgba(0, 255, 0, 0.3);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .slow-btn {
    background: #333;
    color: #fff;
    border: 1px solid #666;
    border-radius: 4px;
    padding: 2px 6px;
    font-size: 10px;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.2s;
  }

  .slow-btn:hover {
    background: #555;
  }

  .slow-btn.active {
    background: #ff0000;
    border-color: #ffcccc;
    color: white;
    box-shadow: 0 0 5px rgba(255, 0, 0, 0.5);
  }
</style>
