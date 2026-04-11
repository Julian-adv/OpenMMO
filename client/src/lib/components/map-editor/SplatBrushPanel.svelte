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
  import { ALL_SPLAT_TEXTURES, loadSplatLayer } from '../../utils/splatLayerLoader'
  import type { LayerConfig } from '../../utils/splatLayerLoader'
  import type { RegionMeta } from '../../managers/terrainMetaManager'
  import { get } from 'svelte/store'

  const LAYER_COLORS = ['#66cc66', '#999999', '#bb7744', '#ddeeff']
  const THUMB_SIZE = 64

  let size = $state(3)
  let strength = $state(8)
  let layer = $state(0)
  let layers = $state<SplatLayerInfo[]>([])
  let configs = $state<LayerConfig[]>([])
  let region = $state<{ rx: number; rz: number } | null>(null)
  let openDropdown = $state<number | null>(null)
  let thumbnails = $state<Record<string, string>>({})

  brushSize.subscribe((v) => (size = v))
  brushStrength.subscribe((v) => (strength = v))
  splatLayer.subscribe((v) => (layer = v))
  currentRegionLayers.subscribe((v) => (layers = v))
  currentRegionConfigs.subscribe((v) => (configs = v))
  currentEditorRegion.subscribe((v) => {
    region = v
    openDropdown = null
  })

  /** Load thumbnails for all splat textures in parallel, single state update */
  async function loadThumbnails() {
    const canvas = document.createElement('canvas')
    canvas.width = THUMB_SIZE
    canvas.height = THUMB_SIZE
    const ctx = canvas.getContext('2d')!

    const layers = await Promise.all(
      ALL_SPLAT_TEXTURES.map((tex) =>
        loadSplatLayer(tex.name, 1).catch(() => null)
      )
    )

    const result: Record<string, string> = {}
    for (let i = 0; i < ALL_SPLAT_TEXTURES.length; i++) {
      const layer = layers[i]
      const img = layer?.map.image as HTMLImageElement | undefined
      if (!img) continue
      ctx.clearRect(0, 0, THUMB_SIZE, THUMB_SIZE)
      ctx.drawImage(img as HTMLImageElement, 0, 0, THUMB_SIZE, THUMB_SIZE)
      result[ALL_SPLAT_TEXTURES[i].name] = canvas.toDataURL('image/jpeg', 0.7)
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
    if (openDropdown !== null && openDropdown !== index) {
      openDropdown = null
    }
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

  <div class="section-label">Brush <span class="hint">(right-click to change texture)</span></div>
  <div class="texture-slots-grid">
    {#each layers as l, i (i)}
      <div class="texture-slot">
        <button
          class="grid-item"
          class:selected={layer === i}
          onclick={() => selectLayer(i)}
          oncontextmenu={(e) => { e.preventDefault(); selectLayer(i); toggleDropdown(i) }}
          title={l.label}
        >
          {#if configs[i] && thumbnails[configs[i].texture]}
            <img class="grid-thumb" src={thumbnails[configs[i].texture]} alt="" />
          {:else}
            <span class="grid-placeholder" style="color: {l.color}">?</span>
          {/if}
          <span class="grid-label">{l.label}</span>
        </button>

        {#if openDropdown === i}
          <div class="dropdown-grid">
            {#each ALL_SPLAT_TEXTURES as tex (tex.name)}
              {@const isActive = configs[i]?.texture === tex.name}
              <button
                class="grid-item"
                class:selected={isActive}
                onclick={() => changeTexture(i, tex.name)}
                title={textureNameToLabel(tex.name)}
              >
                {#if thumbnails[tex.name]}
                  <img class="grid-thumb" src={thumbnails[tex.name]} alt="" />
                {:else}
                  <span class="grid-placeholder">?</span>
                {/if}
                <span class="grid-label">{textureNameToLabel(tex.name)}</span>
                {#if isActive}<span class="grid-check">✓</span>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
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

  .texture-slots-grid {
    display: flex;
    gap: 4px;
    margin-bottom: 8px;
  }

  .texture-slot {
    position: relative;
  }

  .dropdown-grid {
    position: absolute;
    left: 0;
    bottom: 100%;
    z-index: 10;
    background: rgba(20, 20, 20, 0.95);
    border: 1px solid rgba(226, 185, 59, 0.3);
    border-radius: 4px;
    margin-bottom: 2px;
    padding: 4px;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    width: 240px;
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

  .grid-check {
    position: absolute;
    top: 2px;
    right: 3px;
    color: #e2b93b;
    font-size: 11px;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.9);
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
