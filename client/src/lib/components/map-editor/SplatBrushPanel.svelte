<script lang="ts">
  import {
    brushSize,
    brushStrength,
    splatLayer,
    currentRegionLayers,
    currentEditorRegion,
    currentRegionConfigs,
    editorMetaManager,
    regionMetaVersion,
    textureNameToLabel,
    type SplatLayerInfo,
  } from '../../stores/editorStore'
  import { ALL_SPLAT_TEXTURES } from '../../utils/splatLayerLoader'
  import type { LayerConfig } from '../../utils/splatLayerLoader'
  import type { RegionMeta } from '../../managers/terrainMetaManager'
  import { get } from 'svelte/store'

  const LAYER_COLORS = ['#66cc66', '#999999', '#bb7744', '#ddeeff']

  let size = $state(3)
  let strength = $state(5)
  let layer = $state(0)
  let layers = $state<SplatLayerInfo[]>([])
  let configs = $state<LayerConfig[]>([])
  let region = $state<{ rx: number; rz: number } | null>(null)
  let openDropdown = $state<number | null>(null)

  brushSize.subscribe((v) => (size = v))
  brushStrength.subscribe((v) => (strength = v))
  splatLayer.subscribe((v) => (layer = v))
  currentRegionLayers.subscribe((v) => (layers = v))
  currentRegionConfigs.subscribe((v) => (configs = v))
  currentEditorRegion.subscribe((v) => {
    region = v
    openDropdown = null
  })

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

  function toggleDropdown(index: number) {
    openDropdown = openDropdown === index ? null : index
  }

  async function changeTexture(slotIndex: number, textureName: string) {
    const metaManager = get(editorMetaManager)
    if (!metaManager || !region) return

    const tex = ALL_SPLAT_TEXTURES.find((t) => t.name === textureName)
    if (!tex) return

    // Keep existing tileScale if the texture was already in this slot, otherwise use default
    const newConfig: LayerConfig = {
      texture: textureName,
      tileScale: configs[slotIndex]?.texture === textureName
        ? configs[slotIndex].tileScale
        : tex.defaultTileScale,
    }

    const newConfigs = [...configs] as [LayerConfig, LayerConfig, LayerConfig, LayerConfig]
    newConfigs[slotIndex] = newConfig

    const meta: RegionMeta = { layers: newConfigs }
    await metaManager.saveMeta(region.rx, region.rz, meta)

    // Update stores
    currentRegionConfigs.set([...newConfigs])
    currentRegionLayers.set(
      newConfigs.map((l, i) => ({
        label: textureNameToLabel(l.texture),
        color: LAYER_COLORS[i] ?? '#ffffff',
      }))
    )
    regionMetaVersion.update((v) => v + 1)

    openDropdown = null
  }
</script>

<div class="splat-brush-panel">
  <div class="panel-title">Splat Brush</div>

  <div class="section-label">Region Textures</div>
  <div class="texture-slots">
    {#each layers as l, i (i)}
      <div class="texture-slot">
        <button
          class="texture-slot-btn"
          class:active={layer === i}
          onclick={() => { selectLayer(i); toggleDropdown(i) }}
        >
          <span class="color-dot" style="background: {l.color}"></span>
          <span class="slot-label">{l.label}</span>
          <span class="dropdown-arrow">{openDropdown === i ? '▲' : '▼'}</span>
        </button>

        {#if openDropdown === i}
          <div class="dropdown">
            {#each ALL_SPLAT_TEXTURES as tex (tex.name)}
              {@const isActive = configs[i]?.texture === tex.name}
              <button
                class="dropdown-item"
                class:selected={isActive}
                onclick={() => changeTexture(i, tex.name)}
              >
                {textureNameToLabel(tex.name)}
                {#if isActive}<span class="check">✓</span>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <div class="section-label">Brush</div>
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

  .texture-slots {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 8px;
  }

  .texture-slot {
    position: relative;
  }

  .texture-slot-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 8px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.03);
    color: #ccc;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    transition: background 150ms ease, border-color 150ms ease;
    text-align: left;
  }

  .texture-slot-btn:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .texture-slot-btn.active {
    background: rgba(226, 185, 59, 0.15);
    border-color: rgba(226, 185, 59, 0.4);
  }

  .slot-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dropdown-arrow {
    font-size: 8px;
    color: #666;
    flex-shrink: 0;
  }

  .dropdown {
    position: absolute;
    left: 0;
    right: 0;
    top: 100%;
    z-index: 10;
    background: rgba(20, 20, 20, 0.95);
    border: 1px solid rgba(226, 185, 59, 0.3);
    border-radius: 4px;
    overflow: hidden;
    margin-top: 2px;
  }

  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 8px;
    border: none;
    background: transparent;
    color: #bbb;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    text-align: left;
    transition: background 100ms ease;
  }

  .dropdown-item:hover {
    background: rgba(226, 185, 59, 0.15);
    color: #fff;
  }

  .dropdown-item.selected {
    color: #e2b93b;
  }

  .check {
    margin-left: auto;
    font-size: 10px;
    color: #e2b93b;
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
