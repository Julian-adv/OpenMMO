<script lang="ts">
  import { brushSize, brushStrength, brushRaiseMode, cursorHeight } from '../../stores/editorStore'

  let size = $state(3)
  let strength = $state(5)
  let raise = $state(true)
  let height = $state<number | null>(null)

  brushSize.subscribe((v) => (size = v))
  brushStrength.subscribe((v) => (strength = v))
  brushRaiseMode.subscribe((v) => (raise = v))
  cursorHeight.subscribe((v) => (height = v))

  function onSizeChange(event: Event) {
    const value = parseInt((event.target as HTMLInputElement).value)
    brushSize.set(value)
  }

  function onStrengthChange(event: Event) {
    const value = parseFloat((event.target as HTMLInputElement).value)
    brushStrength.set(value)
  }

  function toggleMode() {
    brushRaiseMode.update((v) => !v)
  }
</script>

<div class="height-brush-panel">
  <div class="panel-title">Height Brush</div>

  <div class="control-row">
    <label for="brush-size">Size</label>
    <input
      id="brush-size"
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
    <label for="brush-strength">Strength</label>
    <input
      id="brush-strength"
      type="range"
      min="1"
      max="10"
      step="1"
      value={strength}
      oninput={onStrengthChange}
    />
    <span class="value">{strength.toFixed(1)}</span>
  </div>

  <div class="control-row">
    <button class="mode-btn" class:raise class:lower={!raise} onclick={toggleMode}>
      {raise ? 'Raise' : 'Lower'}
    </button>
    <span class="hint">Shift to invert</span>
  </div>

  {#if height !== null}
    <div class="info-row">
      Height: {height.toFixed(2)}m
    </div>
  {/if}
</div>

<style>
  .height-brush-panel {
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

  .mode-btn {
    padding: 4px 12px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    cursor: pointer;
    font-family: inherit;
    font-size: 12px;
    font-weight: bold;
    transition: background 150ms ease;
  }

  .mode-btn.raise {
    background: rgba(102, 255, 102, 0.2);
    color: #66ff66;
    border-color: rgba(102, 255, 102, 0.4);
  }

  .mode-btn.lower {
    background: rgba(255, 102, 102, 0.2);
    color: #ff6666;
    border-color: rgba(255, 102, 102, 0.4);
  }

  .hint {
    color: #666;
    font-size: 11px;
  }

  .info-row {
    margin-top: 4px;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    color: #ccc;
  }
</style>
