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
    housingEditorTool,
    selectedHouseId,
    selectedRoomIndex,
    wallVariants,
    WALL_VARIANT_OPTIONS,
    type RoomTemplate,
    type WallVariants,
    type HousingEditorTool,
  } from '../../stores/housingEditorStore'
  import type { HouseData, RoomData, WallVariant } from '../../types/housing'
  import { HOUSING_TEXTURES } from '../../utils/housing-textures'
  import { housingManager } from '../../managers/housingManager'

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
  let tool = $state<HousingEditorTool>('place')
  let variants = $state<WallVariants>({
    north: 'solid',
    south: 'door',
    east: 'solid',
    west: 'solid',
  })
  let editHouseId = $state<string | null>(null)
  let editRoomIdx = $state<number | null>(null)
  let editVersion = $state(0)

  // Derived: the room being edited (editVersion forces recompute after local edits)
  let editRoom = $derived.by(() => {
    void editVersion
    if (editHouseId == null || editRoomIdx == null) return null
    const house = housingManager.getHouseById(editHouseId)
    if (!house || editRoomIdx >= house.rooms.length) return null
    return house.rooms[editRoomIdx]
  })

  const unsubs = [
    placementRotation.subscribe((v) => (rotation = v)),
    wallTextureIndex.subscribe((v) => (wallTex = v)),
    floorTextureIndex.subscribe((v) => (floorTex = v)),
    roofTextureIndex.subscribe((v) => (roofTex = v)),
    selectedRoomTemplate.subscribe((v) => (selected = v)),
    placementPreview.subscribe((v) => (preview = v)),
    housingEditorTool.subscribe((v) => (tool = v)),
    wallVariants.subscribe((v) => (variants = v)),
    selectedHouseId.subscribe((v) => (editHouseId = v)),
    selectedRoomIndex.subscribe((v) => (editRoomIdx = v)),
  ]
  onDestroy(() => {
    unsubs.forEach((u) => u())
    if (editSaveTimer) clearTimeout(editSaveTimer)
  })

  function setTool(t: HousingEditorTool) {
    housingEditorTool.set(t)
  }

  function selectTemplate(t: RoomTemplate) {
    housingEditorTool.set('place')
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

  // --- Edit mode functions ---

  let editSaveTimer: ReturnType<typeof setTimeout> | null = null
  let pendingHouse: HouseData | null = null

  function applyRoomEdit(mutateFn: (room: RoomData) => void) {
    if (editHouseId == null || editRoomIdx == null) return

    const house = housingManager.getHouseById(editHouseId)
    if (!house || editRoomIdx >= house.rooms.length) return

    const updatedHouse: HouseData = structuredClone(house)
    mutateFn(updatedHouse.rooms[editRoomIdx])

    // Instant local update for visual feedback
    housingManager.updateLocalCache(updatedHouse)
    editVersion++

    // Debounce server save
    pendingHouse = updatedHouse
    if (editSaveTimer) clearTimeout(editSaveTimer)
    editSaveTimer = setTimeout(async () => {
      if (pendingHouse) {
        await housingManager.updateHouse(pendingHouse)
        pendingHouse = null
      }
    }, 300)
  }

  type WallDirKey = 'wallNorth' | 'wallSouth' | 'wallEast' | 'wallWest'

  const WALL_DIRS: { label: string; wallKey: WallDirKey }[] = [
    { label: 'N', wallKey: 'wallNorth' },
    { label: 'S', wallKey: 'wallSouth' },
    { label: 'E', wallKey: 'wallEast' },
    { label: 'W', wallKey: 'wallWest' },
  ]

  function setSegmentVariant(wallKey: WallDirKey, segIdx: number, variant: WallVariant) {
    applyRoomEdit((room) => {
      room[wallKey][segIdx] = { ...room[wallKey][segIdx], variant }
    })
  }

  function onEditTextureChange(kind: 'wall' | 'floor' | 'roof', idx: number) {
    const store = kind === 'wall' ? wallTextureIndex : kind === 'floor' ? floorTextureIndex : roofTextureIndex
    store.set(idx)
    applyRoomEdit((room) => {
      if (kind === 'wall') {
        for (const { wallKey } of WALL_DIRS) {
          for (const seg of room[wallKey]) {
            if (seg.variant !== 'open') seg.texture = idx
          }
        }
      } else if (kind === 'floor') {
        room.floorTexture = idx
      } else {
        room.roofTexture = idx
      }
    })
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
    <div class="section-title">Tools</div>
    <div class="tool-row">
      <button class="tool-btn" class:active={tool === 'place'} onclick={() => setTool('place')}>
        Place
      </button>
      <button class="tool-btn tool-select" class:active={tool === 'select'} onclick={() => setTool('select')}>
        Select
      </button>
      <button class="tool-btn tool-delete" class:active={tool === 'delete'} onclick={() => setTool('delete')}>
        Delete
      </button>
    </div>

    {#snippet texturePicker(title: string, activeIdx: number, onChange: (idx: number) => void)}
      <div class="section-title">{title}</div>
      <div class="tex-row">
        {#each TEX_ENTRIES as entry, i (i)}
          <button
            class="tex-btn"
            class:active={activeIdx === i}
            style="--swatch-color: {entry.color}"
            onclick={() => onChange(i)}
          >
            <span class="tex-swatch"></span>
            <span class="tex-label">{entry.label}</span>
          </button>
        {/each}
      </div>
    {/snippet}

    {#if tool === 'place'}
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

      {@render texturePicker('Wall', wallTex, (i) => wallTextureIndex.set(i))}
      {@render texturePicker('Floor', floorTex, (i) => floorTextureIndex.set(i))}
      {@render texturePicker('Roof', roofTex, (i) => roofTextureIndex.set(i))}

    {:else if tool === 'select'}
      {#if editRoom && editRoomIdx != null}
        <div class="section-title">Room {editRoomIdx + 1} ({editRoom.sizeX}×{editRoom.sizeZ})</div>

        {#each WALL_DIRS as dir (dir.wallKey)}
          {@const wall = editRoom[dir.wallKey]}
          <div class="section-title">{dir.label} Wall ({wall.length} seg)</div>
          <div class="segment-row">
            {#each wall as seg, segIdx (segIdx)}
              <div class="segment-group">
                {#each WALL_VARIANT_OPTIONS as variant (variant)}
                  <button
                    class="variant-btn"
                    class:active={seg.variant === variant}
                    disabled={seg.variant === 'open'}
                    title="Seg {segIdx + 1} → {variant}"
                    onclick={() => setSegmentVariant(dir.wallKey, segIdx, variant)}
                  >{VARIANT_LABELS[variant]}</button>
                {/each}
                {#if seg.variant === 'open'}
                  <span class="open-label">open</span>
                {/if}
              </div>
            {/each}
          </div>
        {/each}

        {@render texturePicker('Wall', wallTex, (i) => onEditTextureChange('wall', i))}
        {@render texturePicker('Floor', floorTex, (i) => onEditTextureChange('floor', i))}
        {@render texturePicker('Roof', roofTex, (i) => onEditTextureChange('roof', i))}
      {:else}
        <div class="info-text">Click a house to select a room</div>
      {/if}

    {:else if tool === 'delete'}
      <div class="info-text">Click a house to delete it</div>
    {/if}
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
    max-height: calc(100vh - 48px);
    overflow-y: auto;
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

  .tool-row {
    display: flex;
    gap: 3px;
  }

  .tool-btn {
    flex: 1;
    padding: 6px 8px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    color: #aaa;
    cursor: pointer;
    font-family: 'Courier New', monospace;
    font-size: 11px;
    transition: background 150ms ease, color 150ms ease;
  }

  .tool-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #ddd;
  }

  .tool-btn.active {
    background: rgba(123, 198, 123, 0.2);
    border-color: rgba(123, 198, 123, 0.5);
    color: #7bc67b;
  }

  .tool-select.active {
    background: rgba(68, 170, 255, 0.2);
    border-color: rgba(68, 170, 255, 0.5);
    color: #44aaff;
  }

  .tool-delete.active {
    background: rgba(255, 80, 80, 0.2);
    border-color: rgba(255, 80, 80, 0.5);
    color: #ff6666;
  }

  .info-text {
    color: #888;
    font-size: 11px;
    margin-top: 12px;
    text-align: center;
    padding: 8px;
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

  .variant-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
    color: #ddd;
  }

  .variant-btn.active {
    background: rgba(123, 198, 123, 0.2);
    border-color: rgba(123, 198, 123, 0.5);
    color: #7bc67b;
  }

  .variant-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .segment-row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .segment-group {
    display: flex;
    gap: 1px;
    align-items: center;
  }

  .open-label {
    font-size: 9px;
    color: #666;
    margin-left: 2px;
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
</style>
