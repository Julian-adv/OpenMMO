<script lang="ts">
  import { brushSize, brushStrength, splatLayer, currentRegionLayers, type SplatLayerInfo } from '../../stores/editorStore'

  let size = $state(3)
  let strength = $state(5)
  let layer = $state(0)
  let layers = $state<SplatLayerInfo[]>([])

  brushSize.subscribe((v) => (size = v))
  brushStrength.subscribe((v) => (strength = v))
  splatLayer.subscribe((v) => (layer = v))
  currentRegionLayers.subscribe((v) => (layers = v))

  function onSizeChange(event: Event) {
    const value = parseInt((event.target as HTMLInputElement).value)
    brushSize.set(value)
  }

  function onStrengthChange(event: Event) {
    const value = parseFloat((event.target as HTMLInputElement).value)
    brushStrength.set(value)
  }

  function selectLayer(index: number) {
    splatLayer.set(index)
  }
</script>

<div class="splat-brush-panel">
  <div class="panel-title">Splat Brush</div>

  <div class="layer-buttons">
    {#each layers as l, i (i)}
      <button
        class="layer-btn"
        class:active={layer === i}
        style="--layer-color: {l.color}"
        onclick={() => selectLayer(i)}
      >
        <span class="color-dot" style="background: {l.color}"></span>
        {l.label}
      </button>
    {/each}
  </div>

  <div class="control-row">
    <label for="splat-brush-size">Size</label>
    <input
      id="splat-brush-size"
      type="range"
      min="1"
      max="10"
      step="1"
      value={size}
      oninput={onSizeChange}
    />
    <span class="value">{size}</span>
  </div>

  <div class="control-row">
    <label for="splat-brush-strength">Strength</label>
    <input
      id="splat-brush-strength"
      type="range"
      min="1"
      max="10"
      step="1"
      value={strength}
      oninput={onStrengthChange}
    />
    <span class="value">{strength.toFixed(1)}</span>
  </div>
</div>

<style>
  .splat-brush-panel {
    position: fixed;
    left: 16px;
    bottom: 16px;
    z-index: 1000;
    background: rgba(0, 0, 0, 0.85);
    color: #e0e0e0;
    padding: 12px 16px;
    border-radius: 8px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    border: 1px solid rgba(226, 185, 59, 0.3);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.6);
    min-width: 180px;
    user-select: none;
  }

  .panel-title {
    color: #e2b93b;
    font-weight: bold;
    font-size: 13px;
    margin-bottom: 10px;
    letter-spacing: 1px;
  }

  .layer-buttons {
    display: flex;
    gap: 4px;
    margin-bottom: 10px;
    flex-wrap: wrap;
  }

  .layer-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    color: #ccc;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    transition: background 150ms ease, border-color 150ms ease;
  }

  .layer-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .layer-btn.active {
    background: rgba(226, 185, 59, 0.2);
    border-color: rgba(226, 185, 59, 0.6);
    color: #fff;
    font-weight: bold;
  }

  .color-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .control-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .control-row label {
    width: 60px;
    flex-shrink: 0;
    color: #aaa;
  }

  .control-row input[type='range'] {
    flex: 1;
    accent-color: #e2b93b;
    height: 4px;
  }

  .value {
    width: 32px;
    text-align: right;
    color: #fff;
    font-weight: bold;
  }
</style>
