<script lang="ts">
  import {
    brushSize,
    brushStrength,
    splatLayer,
    textureNameToLabel,
  } from '../../stores/editorStore'
  import { GLOBAL_PALETTE, loadSplatLayer } from '../../utils/splatLayerLoader'

  interface Props {
    title?: string
    hint?: string
  }
  let {
    title = 'Splat Brush',
    hint = '(click to select slot)',
  }: Props = $props()

  // One color swatch per global-palette slot — purely cosmetic fallback when
  // a texture thumbnail hasn't loaded yet. Repeated for slot count > array length.
  const SLOT_SWATCH_COLORS = [
    '#66cc66', // 0 grass
    '#d9ba6e', // 1 sand
    '#b06438', // 2 laterite
    '#ddeeff', // 3 snow
    '#8c877d', // 4 road
    '#c9c4ba', // 5 cliff
  ]
  const THUMB_SIZE = 64

  let size = $state(3)
  let strength = $state(8)
  let layer = $state(0)
  let thumbnails = $state<Record<string, string>>({})

  brushSize.subscribe((v) => (size = v))
  brushStrength.subscribe((v) => (strength = v))
  splatLayer.subscribe((v) => (layer = v))

  async function loadThumbnails() {
    const canvas = document.createElement('canvas')
    canvas.width = THUMB_SIZE
    canvas.height = THUMB_SIZE
    const ctx = canvas.getContext('2d')!

    const loaded = await Promise.all(
      GLOBAL_PALETTE.map((cfg) =>
        loadSplatLayer(cfg.texture, 1).catch(() => null)
      )
    )

    const result: Record<string, string> = {}
    for (let i = 0; i < GLOBAL_PALETTE.length; i++) {
      const l = loaded[i]
      const img = l?.map.image as HTMLImageElement | undefined
      if (!img) continue
      ctx.clearRect(0, 0, THUMB_SIZE, THUMB_SIZE)
      ctx.drawImage(img as HTMLImageElement, 0, 0, THUMB_SIZE, THUMB_SIZE)
      result[GLOBAL_PALETTE[i].texture] = canvas.toDataURL('image/jpeg', 0.7)
    }
    thumbnails = result
  }

  loadThumbnails()

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
  <div class="panel-title">{title}</div>

  <div class="section-label">Brush <span class="hint">{hint}</span></div>
  <div class="palette-grid">
    {#each GLOBAL_PALETTE as cfg, i (cfg.texture)}
      {@const label = textureNameToLabel(cfg.texture)}
      {@const swatch = SLOT_SWATCH_COLORS[i % SLOT_SWATCH_COLORS.length]}
      <button
        class="grid-item"
        class:selected={layer === i}
        onclick={() => selectLayer(i)}
        title={label}
      >
        {#if thumbnails[cfg.texture]}
          <img class="grid-thumb" src={thumbnails[cfg.texture]} alt="" />
        {:else}
          <span class="grid-placeholder" style="color: {swatch}">?</span>
        {/if}
        <span class="grid-label">{label}</span>
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
    background: rgba(0, 0, 0, 0.85);
    color: #e0e0e0;
    padding: 12px 16px;
    border-radius: 8px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    border: 1px solid rgba(226, 185, 59, 0.3);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.6);
    min-width: 200px;
    user-select: none;
  }

  .panel-title {
    color: #e2b93b;
    font-weight: bold;
    font-size: 13px;
    margin-bottom: 10px;
    letter-spacing: 1px;
  }

  .section-label {
    color: #888;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-bottom: 4px;
    margin-top: 8px;
  }

  .section-label:first-of-type {
    margin-top: 0;
  }

  .hint {
    color: #666;
    font-size: 9px;
    text-transform: none;
    letter-spacing: 0;
  }

  .palette-grid {
    display: grid;
    grid-template-columns: repeat(8, 64px);
    gap: 4px;
    margin-bottom: 8px;
  }

  .grid-item {
    position: relative;
    width: 64px;
    border: 2px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    cursor: pointer;
    padding: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    transition: border-color 150ms ease;
  }

  .grid-item:hover {
    border-color: rgba(226, 185, 59, 0.5);
  }

  .grid-item.selected {
    border-color: #e2b93b;
  }

  .grid-thumb {
    display: block;
    width: 64px;
    height: 64px;
    object-fit: cover;
  }

  .grid-placeholder {
    width: 64px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #555;
    font-size: 18px;
  }

  .grid-label {
    position: relative;
    z-index: 1;
    width: 100%;
    padding: 2px 3px;
    background: rgba(0, 0, 0, 0.7);
    color: #ddd;
    font-family: inherit;
    font-size: 9px;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
