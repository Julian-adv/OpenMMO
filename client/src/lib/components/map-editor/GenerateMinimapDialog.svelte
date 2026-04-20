<script lang="ts">
  import { get } from 'svelte/store'
  import {
    showMinimapDialog,
    minimapVersion,
  } from '../../stores/editorStore'
  import { generateRegionMinimap } from '../../terrain/regionMinimapGenerator'

  let generating = $state(false)
  let progress = $state(0)
  let progressLabel = $state('')

  function close() {
    showMinimapDialog.set(null)
  }

  async function handleGenerate() {
    const region = get(showMinimapDialog)
    if (!region) return

    generating = true
    progress = 0
    progressLabel = 'Starting...'

    try {
      const _blob = await generateRegionMinimap(
        region.rx,
        region.rz,
        (pct, label) => {
          progress = pct
          progressLabel = label
        }
      )

      // Bust cached img in WorldMapDialog
      minimapVersion.update((v) => v + 1)

      progress = 100
      progressLabel = 'Done!'

      await new Promise((r) => setTimeout(r, 300))
      close()
    } catch (e) {
      console.error('Minimap generation failed:', e)
      progressLabel = `Error: ${e instanceof Error ? e.message : String(e)}`
    } finally {
      generating = false
    }
  }
</script>

<div class="backdrop" role="dialog" aria-modal="true">
  <div class="dialog">
    <h2>Generate Region Minimap <span class="region-label">Region ({$showMinimapDialog?.rx}, {$showMinimapDialog?.rz})</span></h2>

    {#if !generating}
      <p class="description">
        Generate a 1024×1024 minimap for the current region.
        Each cell (1m) becomes 1 pixel, colored by terrain type and water depth.
      </p>

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

  .region-label {
    font-size: 12px;
    color: #aaa;
    font-weight: normal;
    letter-spacing: 0;
  }

  .description {
    font-size: 12px;
    color: #aaa;
    margin: 0 0 16px 0;
    line-height: 1.5;
  }

  .actions {
    display: flex;
    gap: 10px;
    justify-content: center;
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
