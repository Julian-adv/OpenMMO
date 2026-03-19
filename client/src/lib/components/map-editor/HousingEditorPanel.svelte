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
    type RoomTemplate,
  } from '../../stores/housingEditorStore'
  import {
    WALL_COLORS as WALL_HEX,
    FLOOR_COLORS as FLOOR_HEX,
    ROOF_COLORS as ROOF_HEX,
  } from '../../utils/house-geometry'

  const toCSS = (c: number) => `#${c.toString(16).padStart(6, '0')}`
  const WALL_COLORS = WALL_HEX.map(toCSS)
  const FLOOR_COLORS = FLOOR_HEX.map(toCSS)
  const ROOF_COLORS = ROOF_HEX.map(toCSS)

  let rotation = $state(0)
  let wallTex = $state(0)
  let floorTex = $state(0)
  let roofTex = $state(0)
  let selected = $state<RoomTemplate | null>(null)
  let preview = $state<{ x: number; z: number } | null>(null)
  let deleteMode = $state(false)

  const unsubs = [
    placementRotation.subscribe((v) => (rotation = v)),
    wallTextureIndex.subscribe((v) => (wallTex = v)),
    floorTextureIndex.subscribe((v) => (floorTex = v)),
    roofTextureIndex.subscribe((v) => (roofTex = v)),
    selectedRoomTemplate.subscribe((v) => (selected = v)),
    placementPreview.subscribe((v) => (preview = v)),
    housingDeleteMode.subscribe((v) => (deleteMode = v)),
  ]
  onDestroy(() => unsubs.forEach((u) => u()))

  function selectTemplate(t: RoomTemplate) {
    housingDeleteMode.set(false)
    selectedRoomTemplate.set(t)
  }

  function rotate() {
    placementRotation.set((rotation + 90) % 360)
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

    <div class="section-title">Wall</div>
    <div class="color-row">
      {#each WALL_COLORS as color, i (i)}
        <button
          class="color-swatch"
          class:active={wallTex === i}
          style="background: {color}"
          aria-label="Wall color {i + 1}"
          onclick={() => wallTextureIndex.set(i)}
        ></button>
      {/each}
    </div>

    <div class="section-title">Floor</div>
    <div class="color-row">
      {#each FLOOR_COLORS as color, i (i)}
        <button
          class="color-swatch"
          class:active={floorTex === i}
          style="background: {color}"
          aria-label="Floor color {i + 1}"
          onclick={() => floorTextureIndex.set(i)}
        ></button>
      {/each}
    </div>

    <div class="section-title">Roof</div>
    <div class="color-row">
      {#each ROOF_COLORS as color, i (i)}
        <button
          class="color-swatch"
          class:active={roofTex === i}
          style="background: {color}"
          aria-label="Roof color {i + 1}"
          onclick={() => roofTextureIndex.set(i)}
        ></button>
      {/each}
    </div>

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
    min-width: 180px;
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

  .color-row {
    display: flex;
    gap: 4px;
  }

  .color-swatch {
    width: 24px;
    height: 24px;
    border-radius: 4px;
    border: 2px solid transparent;
    cursor: pointer;
    transition: border-color 150ms ease;
  }

  .color-swatch:hover {
    border-color: rgba(255, 255, 255, 0.4);
  }

  .color-swatch.active {
    border-color: #7bc67b;
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
