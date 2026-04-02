<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import {
    npcNames,
    selectedNpc,
    selectedNpcSchedule,
    selectedScheduleIndex,
  } from '../../stores/editorStore'
  import { NpcScheduleManager } from '../../managers/npcScheduleManager'
  import type { NpcScheduleData } from '../../managers/npcScheduleManager'

  let names = $state<string[]>([])
  let npc = $state<string | null>(null)
  let schedule = $state<NpcScheduleData | null>(null)
  let schedIdx = $state(0)
  let manager: NpcScheduleManager | null = null
  let saving = $state(false)

  const unsubs = [
    npcNames.subscribe((v) => (names = v)),
    selectedNpc.subscribe((v) => (npc = v)),
    selectedNpcSchedule.subscribe((v) => (schedule = v)),
    selectedScheduleIndex.subscribe((v) => (schedIdx = v)),
  ]
  onDestroy(() => unsubs.forEach((u) => u()))

  onMount(async () => {
    if (get(npcNames).length > 0) return
    manager = new NpcScheduleManager()
    const list = await manager.listNpcs()
    npcNames.set(list)
  })

  function selectEntry(index: number) {
    selectedScheduleIndex.set(index)
  }

  async function selectNpcByName(name: string) {
    selectedNpcSchedule.set(null)
    selectedScheduleIndex.set(0)
    selectedNpc.set(name)
    if (!manager) manager = new NpcScheduleManager()
    try {
      const data = await manager.fetchSchedule(name)
      if (get(selectedNpc) === name) {
        selectedNpcSchedule.set(data)
      }
    } catch (e) {
      console.error(`Failed to fetch schedule for '${name}':`, e)
    }
  }

  function formatPos(pos: [number, number, number]): string {
    return `${pos[0].toFixed(1)}, ${pos[1].toFixed(1)}, ${pos[2].toFixed(1)}`
  }

  async function save() {
    if (!manager || !npc || !schedule) return
    saving = true
    try {
      await manager.saveSchedule(npc, schedule)
    } finally {
      saving = false
    }
  }

  function addWaypoint() {
    if (!schedule) return
    const entry = schedule.schedule[schedIdx]
    if (!entry) return
    // Add waypoint at home position
    const newWp: [number, number, number] = [...entry.pos]
    const updated = structuredClone(schedule)
    const updatedEntry = updated.schedule[schedIdx]
    updatedEntry.waypoints.push(newWp)
    selectedNpcSchedule.set(updated)
  }

  function removeWaypoint(wpIdx: number) {
    if (!schedule) return
    const updated = structuredClone(schedule)
    const updatedEntry = updated.schedule[schedIdx]
    updatedEntry.waypoints.splice(wpIdx, 1)
    selectedNpcSchedule.set(updated)
  }

  let entry = $derived(schedule?.schedule[schedIdx] ?? null)
</script>

<div class="npc-panel">
  <div class="panel-title">NPC Waypoints</div>

  {#if !npc}
    <div class="draw-hint">Click an NPC in the scene or select from list</div>
    {#if names.length > 0}
      <div class="section-label">Known NPCs</div>
      <div class="npc-list">
        {#each names as name (name)}
          <button class="npc-name-btn" onclick={() => selectNpcByName(name)}>{name}</button>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="selected-npc">{npc}</div>

    {#if schedule}
      <div class="section-label">Schedules</div>
      <div class="schedule-list">
        {#each schedule.schedule as sched, i (i)}
          <button
            class="schedule-item"
            class:active={i === schedIdx}
            onclick={() => selectEntry(i)}
          >
            <span class="sched-at">{sched.at}</span>
            {#if sched.label}
              <span class="sched-label">{sched.label}</span>
            {/if}
          </button>
        {/each}
      </div>

      {#if entry}
        <div class="section-label">Home Position</div>
        <div class="coord-row home">
          <span class="wp-num">#0</span>
          <span class="coord-text">{formatPos(entry.pos)}</span>
        </div>

        <div class="section-label">
          Waypoints
          <button class="add-btn" onclick={addWaypoint}>+</button>
        </div>
        <div class="waypoint-list">
          {#each entry.waypoints as wp, i (i)}
            <div class="coord-row waypoint">
              <span class="wp-num">#{i + 1}</span>
              <span class="coord-text">{formatPos(wp)}</span>
              <button class="delete-btn" onclick={() => removeWaypoint(i)}>x</button>
            </div>
          {/each}
          {#if entry.waypoints.length === 0}
            <div class="empty-msg">No waypoints</div>
          {/if}
        </div>
      {/if}

      <button class="save-btn" onclick={save} disabled={saving}>
        {saving ? 'Saving...' : 'Save'}
      </button>
    {/if}

    <button class="deselect-btn" onclick={() => { selectedNpc.set(null); selectedNpcSchedule.set(null) }}>
      Deselect
    </button>
  {/if}
</div>

<style>
  .npc-panel {
    background: rgba(0, 0, 0, 0.85);
    color: #e0e0e0;
    padding: 12px 16px;
    border-radius: 8px;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    border: 1px solid rgba(226, 185, 59, 0.3);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.6);
    min-width: 240px;
    user-select: none;
  }

  .panel-title {
    color: #e2b93b;
    font-weight: bold;
    font-size: 13px;
    margin-bottom: 10px;
    letter-spacing: 1px;
  }

  .selected-npc {
    color: #44ccff;
    font-weight: bold;
    font-size: 14px;
    margin-bottom: 8px;
    text-transform: capitalize;
  }

  .section-label {
    color: #888;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-bottom: 4px;
    margin-top: 8px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .npc-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .npc-name-btn {
    padding: 3px 6px;
    font-size: 11px;
    color: #999;
    text-transform: capitalize;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 3px;
    cursor: pointer;
    font-family: inherit;
    text-align: left;
    width: 100%;
  }

  .npc-name-btn:hover {
    color: #ccc;
    background: rgba(255, 255, 255, 0.1);
  }

  .schedule-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 100px;
    overflow-y: auto;
  }

  .schedule-item {
    display: flex;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 3px;
    cursor: pointer;
    font-size: 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
  }

  .schedule-item:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .schedule-item.active {
    background: rgba(68, 204, 255, 0.15);
    border-color: rgba(68, 204, 255, 0.4);
  }

  .sched-at {
    color: #e2b93b;
    font-weight: bold;
    flex-shrink: 0;
  }

  .sched-label {
    color: #aaa;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .waypoint-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 120px;
    overflow-y: auto;
  }

  .coord-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 3px;
    font-size: 10px;
  }

  .coord-row.home {
    background: rgba(226, 185, 59, 0.15);
    border: 1px solid rgba(226, 185, 59, 0.3);
  }

  .coord-row.waypoint {
    background: rgba(68, 204, 255, 0.1);
    border: 1px solid rgba(68, 204, 255, 0.2);
  }

  .wp-num {
    color: #e2b93b;
    font-weight: bold;
    width: 24px;
    flex-shrink: 0;
  }

  .coord-text {
    flex: 1;
    color: #ccc;
  }

  .add-btn {
    background: rgba(68, 204, 255, 0.3);
    border: none;
    color: #44ccff;
    cursor: pointer;
    border-radius: 3px;
    padding: 0 5px;
    font-family: inherit;
    font-size: 10px;
    font-weight: bold;
    line-height: 1;
  }

  .add-btn:hover {
    background: rgba(68, 204, 255, 0.5);
  }

  .delete-btn {
    background: rgba(255, 60, 60, 0.3);
    border: none;
    color: #ff6666;
    cursor: pointer;
    border-radius: 3px;
    padding: 1px 5px;
    font-family: inherit;
    font-size: 10px;
    font-weight: bold;
  }

  .delete-btn:hover {
    background: rgba(255, 60, 60, 0.5);
  }

  .save-btn {
    margin-top: 10px;
    width: 100%;
    padding: 6px;
    background: rgba(68, 204, 255, 0.2);
    border: 1px solid rgba(68, 204, 255, 0.4);
    border-radius: 4px;
    color: #44ccff;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    font-weight: bold;
  }

  .save-btn:hover:not(:disabled) {
    background: rgba(68, 204, 255, 0.35);
  }

  .save-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .deselect-btn {
    margin-top: 4px;
    width: 100%;
    padding: 4px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    color: #888;
    cursor: pointer;
    font-family: inherit;
    font-size: 10px;
  }

  .deselect-btn:hover {
    color: #ccc;
    background: rgba(255, 255, 255, 0.1);
  }

  .draw-hint {
    padding: 6px 8px;
    background: rgba(226, 185, 59, 0.1);
    border: 1px solid rgba(226, 185, 59, 0.2);
    border-radius: 4px;
    color: #ccc;
    font-size: 10px;
    text-align: center;
  }

  .empty-msg {
    color: #555;
    font-size: 10px;
    font-style: italic;
    padding: 4px 0;
  }
</style>
