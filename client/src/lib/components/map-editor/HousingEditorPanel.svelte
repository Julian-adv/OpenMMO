<script lang="ts">
  import { onDestroy } from 'svelte'
  import {
    ROOM_TEMPLATES,
    selectedRoomTemplate,
    placementRotation,
    wallTextureIndex,
    floorTextureIndex,
    roofTextureIndex,
    placementPreview,
    housingDeleteMode,
    wallVariants,
    WALL_VARIANT_OPTIONS,
    type RoomTemplate,
    type WallVariants,
  } from '../../stores/housingEditorStore'
  import type { WallVariant } from '../../types/housing'
  import type { Writable } from 'svelte/store'
  import { HOUSING_TEXTURES } from '../../utils/housing-textures'

  const toCSS = (c: number) => `#${c.toString(16).padStart(6, '0')}`
  const TEX_ENTRIES = HOUSING_TEXTURES.map((t) => ({
    label: t.label,
    color: toCSS(t.fallbackColor),
  }))

  let rotation = $state(0)
  let wallTex = $state(0)
  let floorTex = $state(0)
  let roofTex = $state(0)
  let selected = $state<RoomTemplate | null>(null)
  let preview = $state<{ x: number; z: number } | null>(null)
  let deleteMode = $state(false)
  let variants = $state<WallVariants>({
    north: 'solid',
    south: 'door',
    east: 'solid',
    west: 'solid',
  })

  const unsubs = [
    placementRotation.subscribe((v) => (rotation = v)),
    wallTextureIndex.subscribe((v) => (wallTex = v)),
    floorTextureIndex.subscribe((v) => (floorTex = v)),
    roofTextureIndex.subscribe((v) => (roofTex = v)),
    selectedRoomTemplate.subscribe((v) => (selected = v)),
    placementPreview.subscribe((v) => (preview = v)),
    housingDeleteMode.subscribe((v) => (deleteMode = v)),
    wallVariants.subscribe((v) => (variants = v)),
  ]
  onDestroy(() => unsubs.forEach((u) => u()))

  function selectTemplate(t: RoomTemplate) {
    housingDeleteMode.set(false)
    selectedRoomTemplate.set(t)
  }

  function rotate() {
    placementRotation.set((rotation + 90) % 360)
  }

  type WallDir = keyof WallVariants

  const VARIANT_LABELS: Record<string, string> = {
    solid: '⬜',
    door: '🚪',
    window: '⊞',
  }

  function setWallVariant(dir: WallDir, variant: WallVariant) {
    if (variants[dir] === variant) return
    wallVariants.update((v) => ({ ...v, [dir]: variant }))
  }

  function toggleDeleteMode() {
    const next = !deleteMode
    housingDeleteMode.set(next)
    if (next) {
      selectedRoomTemplate.set(null)
    }
  }
</script>

<div class="editor-mode-badge">
  HOUSING{#if preview}
    <span class="cell-info">
      ({preview.x.toFixed(0)}, {preview.z.toFixed(0)})
    </span>
  {/if}
</div>
<div class="editor-panel-container">
  <div class="panel">
    <div class="section-title">Room</div>
    <div class="room-grid">
      {#each ROOM_TEMPLATES as t (t.label)}
        <button
          class="room-btn"
          class:active={selected === t}
          onclick={() => selectTemplate(t)}
        >
          <span class="room-size">{t.sizeX}×{t.sizeZ}</span>
          <span class="room-label">{t.label.split('(')[0].trim()}</span>
        </button>
      {/each}
    </div>

    <div class="section-title">Rotate <span class="hint">(R)</span></div>
    <button class="rotate-btn" onclick={rotate}>{rotation}°</button>

    {#snippet wallButtons(dir: WallDir, label: string)}
      {#each WALL_VARIANT_OPTIONS as variant (variant)}
        <button
          class="variant-btn"
          class:active={variants[dir] === variant}
          title="{label} → {variant}"
          onclick={() => setWallVariant(dir, variant)}
        >{VARIANT_LABELS[variant]}</button>
      {/each}
    {/snippet}

    <div class="section-title">Walls</div>
    <div class="wall-cross">
      <div class="wall-cross-top">{@render wallButtons('north', 'N')}</div>
      <div class="wall-cross-mid">
        <div class="wall-cross-side">{@render wallButtons('west', 'W')}</div>
        <span class="wall-cross-center">+</span>
        <div class="wall-cross-side">{@render wallButtons('east', 'E')}</div>
      </div>
      <div class="wall-cross-bot">{@render wallButtons('south', 'S')}</div>
    </div>

    {#snippet texturePicker(title: string, activeIdx: number, store: Writable<number>)}
      <div class="section-title">{title}</div>
      <div class="tex-row">
        {#each TEX_ENTRIES as entry, i (i)}
          <button
            class="tex-btn"
            class:active={activeIdx === i}
            style="--swatch-color: {entry.color}"
            onclick={() => store.set(i)}
          >
            <span class="tex-swatch"></span>
            <span class="tex-label">{entry.label}</span>
          </button>
        {/each}
      </div>
    {/snippet}

    {@render texturePicker('Wall', wallTex, wallTextureIndex)}
    {@render texturePicker('Floor', floorTex, floorTextureIndex)}
    {@render texturePicker('Roof', roofTex, roofTextureIndex)}

    <div class="section-title">Tools</div>
    <button class="delete-btn" class:active={deleteMode} onclick={toggleDeleteMode}>
      Delete
    </button>
  </div>
</div>

<style>
  .editor-mode-badge {
    position: fixed;
    top: 10px;
    right: 10px;
    z-index: 1000;
    background: rgba(0, 0, 0, 0.8);
    color: #7bc67b;
    padding: 6px 12px;
    border-radius: 6px;
    font-family: 'Courier New', monospace;
    font-size: 13px;
    font-weight: bold;
    border: 1px solid rgba(123, 198, 123, 0.4);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
    pointer-events: none;
    letter-spacing: 1px;
  }

  .cell-info {
    margin-left: 8px;
    color: #ccc;
    font-weight: normal;
    letter-spacing: 0;
  }

  .editor-panel-container {
    position: fixed;
    left: 16px;
    bottom: 16px;
    z-index: 1000;
  }

  .panel {
    background: rgba(0, 0, 0, 0.85);
    border-radius: 8px;
    padding: 10px 12px;
    border: 1px solid rgba(123, 198, 123, 0.3);
    font-family: 'Courier New', monospace;
    font-size: 12px;
    color: #ccc;
    min-width: 240px;
  }

  .section-title {
    color: #7bc67b;
    font-weight: bold;
    font-size: 11px;
    margin-top: 8px;
    margin-bottom: 4px;
    letter-spacing: 0.5px;
  }

  .section-title:first-child {
    margin-top: 0;
  }

  .hint {
    color: #666;
    font-weight: normal;
  }

  .room-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 3px;
  }

  .room-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 6px 4px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    color: #aaa;
    cursor: pointer;
    font-family: 'Courier New', monospace;
    font-size: 11px;
    transition: background 150ms ease, color 150ms ease;
  }

  .room-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #ddd;
  }

  .room-btn.active {
    background: rgba(123, 198, 123, 0.2);
    border-color: rgba(123, 198, 123, 0.5);
    color: #7bc67b;
  }

  .room-size {
    font-size: 14px;
    font-weight: bold;
  }

  .room-label {
    font-size: 9px;
    opacity: 0.7;
  }

  .rotate-btn {
    padding: 4px 12px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    color: #aaa;
    cursor: pointer;
    font-family: 'Courier New', monospace;
    font-size: 12px;
  }

  .rotate-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .wall-cross {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  .wall-cross-top,
  .wall-cross-bot {
    display: flex;
    gap: 2px;
  }

  .wall-cross-mid {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .wall-cross-side {
    display: flex;
    gap: 2px;
  }

  .wall-cross-center {
    font-size: 14px;
    color: #555;
    width: 16px;
    text-align: center;
  }

  .variant-btn {
    width: 26px;
    height: 26px;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.05);
    color: #aaa;
    cursor: pointer;
    font-size: 12px;
    text-align: center;
    line-height: 26px;
    transition: background 150ms ease, color 150ms ease;
  }

  .variant-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #ddd;
  }

  .variant-btn.active {
    background: rgba(123, 198, 123, 0.2);
    border-color: rgba(123, 198, 123, 0.5);
    color: #7bc67b;
  }

  .tex-row {
    display: flex;
    gap: 3px;
  }

  .tex-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 4px 6px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    color: #aaa;
    cursor: pointer;
    font-family: 'Courier New', monospace;
    font-size: 9px;
    transition: background 150ms ease, color 150ms ease;
  }

  .tex-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #ddd;
  }

  .tex-btn.active {
    background: rgba(123, 198, 123, 0.2);
    border-color: rgba(123, 198, 123, 0.5);
    color: #7bc67b;
  }

  .tex-swatch {
    display: block;
    width: 20px;
    height: 20px;
    border-radius: 3px;
    background: var(--swatch-color);
  }

  .tex-label {
    white-space: nowrap;
  }

  .delete-btn {
    width: 100%;
    padding: 6px 12px;
    border: 1px solid rgba(255, 80, 80, 0.3);
    border-radius: 4px;
    background: rgba(255, 80, 80, 0.1);
    color: #cc7777;
    cursor: pointer;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    transition: background 150ms ease, color 150ms ease;
  }

  .delete-btn:hover {
    background: rgba(255, 80, 80, 0.2);
    color: #ff6666;
  }

  .delete-btn.active {
    background: rgba(255, 80, 80, 0.3);
    border-color: rgba(255, 80, 80, 0.6);
    color: #ff4444;
  }
</style>
