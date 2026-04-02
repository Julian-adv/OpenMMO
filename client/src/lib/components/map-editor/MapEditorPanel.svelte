<script lang="ts">
  import { hoveredCell, editorTool, showGenerateDialog, currentEditorRegion } from '../../stores/editorStore'
  import { get } from 'svelte/store'
  import { playerDebugInfo } from '../../stores/debugStore'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'
  import { tileToRegion } from '../../managers/terrainMetaManager'
  import HeightBrushPanel from './HeightBrushPanel.svelte'
  import SplatBrushPanel from './SplatBrushPanel.svelte'
  import ZoneBrushPanel from './ZoneBrushPanel.svelte'
  import NpcBrushPanel from './NpcBrushPanel.svelte'
  import FurnitureBrushPanel from './FurnitureBrushPanel.svelte'

  function getPlayerRegion(): { rx: number; rz: number } | null {
    const info = get(playerDebugInfo)
    if (!info) return null
    const tileX = Math.round(info.position.x / TERRAIN_TILE_SIZE)
    const tileZ = Math.round(info.position.z / TERRAIN_TILE_SIZE)
    return { rx: tileToRegion(tileX), rz: tileToRegion(tileZ) }
  }

  function openGenerateDialog() {
    const region = getPlayerRegion()
    if (region) {
      showGenerateDialog.set({ rx: region.rx, rz: region.rz })
    }
  }
</script>

<div class="editor-mode-badge">
  MAP EDITOR{#if $hoveredCell}
    <span class="cell-info">
      {#if $currentEditorRegion}R({$currentEditorRegion.rx}, {$currentEditorRegion.rz}){/if}
      T({$hoveredCell.tileX}, {$hoveredCell.tileZ})
      C({$hoveredCell.cellX}, {$hoveredCell.cellZ})
    </span>
  {/if}
</div>
<div class="editor-panel-container">
  <div class="editor-tool-tabs">
    <button
      class="tool-tab"
      class:active={$editorTool === 'height'}
      onclick={() => editorTool.set('height')}
    >Height</button>
    <button
      class="tool-tab"
      class:active={$editorTool === 'splat'}
      onclick={() => editorTool.set('splat')}
    >Splat</button>
    <button
      class="tool-tab"
      class:active={$editorTool === 'zone'}
      onclick={() => editorTool.set('zone')}
    >Zone</button>
    <button
      class="tool-tab"
      class:active={$editorTool === 'npc'}
      onclick={() => editorTool.set('npc')}
    >NPC</button>
    <button
      class="tool-tab"
      class:active={$editorTool === 'furniture'}
      onclick={() => editorTool.set('furniture')}
    >Furniture</button>
    <button
      class="tool-tab generate-btn"
      onclick={openGenerateDialog}
    >Generate</button>
  </div>
  {#if $editorTool === 'height'}
    <HeightBrushPanel />
  {:else if $editorTool === 'splat'}
    <SplatBrushPanel />
  {:else if $editorTool === 'zone'}
    <ZoneBrushPanel />
  {:else if $editorTool === 'npc'}
    <NpcBrushPanel />
  {:else if $editorTool === 'furniture'}
    <FurnitureBrushPanel />
  {/if}
</div>

<style>
  .editor-mode-badge {
    position: fixed;
    top: 10px;
    right: 10px;
    z-index: 1000;
    background: rgba(0, 0, 0, 0.8);
    color: #e2b93b;
    padding: 6px 12px;
    border-radius: 6px;
    font-family: 'Courier New', monospace;
    font-size: 13px;
    font-weight: bold;
    border: 1px solid rgba(226, 185, 59, 0.4);
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
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .editor-tool-tabs {
    display: flex;
    gap: 2px;
    background: rgba(0, 0, 0, 0.7);
    border-radius: 6px;
    padding: 3px;
    border: 1px solid rgba(226, 185, 59, 0.3);
    width: fit-content;
  }

  .tool-tab {
    padding: 5px 14px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: #888;
    cursor: pointer;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    font-weight: bold;
    letter-spacing: 0.5px;
    transition: background 150ms ease, color 150ms ease;
  }

  .tool-tab:hover {
    color: #ccc;
  }

  .tool-tab.active {
    background: rgba(226, 185, 59, 0.25);
    color: #e2b93b;
  }

  .tool-tab.generate-btn {
    margin-left: 4px;
    border-left: 1px solid rgba(255, 255, 255, 0.15);
    padding-left: 14px;
  }
</style>
