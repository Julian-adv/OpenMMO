<script lang="ts">
  import { Canvas } from '@threlte/core'
  import type { AccountCharacter, CharacterAttributes } from '../network/socket'
  import CharacterSelectScene from './CharacterSelectScene.svelte'

  const MAX_CHARACTER_SLOTS = 3

  interface Props {
    accountName: string
    characters: AccountCharacter[]
    onRollCharacterStats: () => Promise<{
      ok: boolean
      message?: string
      attributes?: CharacterAttributes
    }>
    onCreateCharacter: (
      characterName: string
    ) => Promise<{ ok: boolean; message?: string; character?: AccountCharacter }>
    onStartGame: (characterId: number) => Promise<{ ok: boolean; message?: string }>
    onDeleteCharacter: (
      characterId: number
    ) => Promise<{ ok: boolean; message?: string }>
    onLogout: () => void
  }

  let {
    accountName,
    characters,
    onRollCharacterStats,
    onCreateCharacter,
    onStartGame,
    onDeleteCharacter,
    onLogout,
  }: Props = $props()

  let selectedCharacterId = $state<number | null>(null)
  let createCharacterName = $state('')
  let rolledAttributes = $state<CharacterAttributes | null>(null)
  let viewMode = $state<'select' | 'create'>('select')
  let isCreating = $state(false)
  let isRolling = $state(false)
  let isStarting = $state(false)
  let isDeleting = $state(false)
  let errorMessage = $state('')

  function isBusy() {
    return isCreating || isRolling || isStarting || isDeleting
  }

  $effect(() => {
    const selectedStillExists = selectedCharacterId
      ? characters.some((character) => character.id === selectedCharacterId)
      : false
    if (!selectedStillExists) {
      selectedCharacterId = characters.length > 0 ? characters[0].id : null
    }
    if (characters.length >= MAX_CHARACTER_SLOTS && viewMode === 'create') {
      viewMode = 'select'
    }
  })

  function handleSlotClick(slotIndex: number) {
    if (isBusy()) return

    const character = characters[slotIndex]
    errorMessage = ''
    if (character) {
      selectedCharacterId = character.id
      viewMode = 'select'
      return
    }

    if (characters.length >= MAX_CHARACTER_SLOTS) {
      errorMessage = 'A maximum of 3 characters can be created.'
      return
    }

    createCharacterName = ''
    rolledAttributes = null
    viewMode = 'create'
  }

  async function handleRoll() {
    if (isBusy()) return

    isRolling = true
    errorMessage = ''
    const result = await onRollCharacterStats()
    isRolling = false

    if (!result.ok) {
      errorMessage = result.message ?? 'Failed to roll character attributes'
      return
    }

    rolledAttributes = result.attributes ?? null
  }

  async function submitCreateCharacter(event: Event) {
    event.preventDefault()
    if (isBusy()) return

    const characterName = createCharacterName.trim()
    if (!characterName) {
      errorMessage = 'Please enter character name'
      return
    }
    if (!rolledAttributes) {
      errorMessage = 'Roll attributes first'
      return
    }

    isCreating = true
    errorMessage = ''
    const result = await onCreateCharacter(characterName)
    isCreating = false

    if (!result.ok) {
      errorMessage = result.message ?? 'Failed to create character'
      return
    }

    if (result.character) {
      selectedCharacterId = result.character.id
    }
    createCharacterName = ''
    rolledAttributes = null
    viewMode = 'select'
  }

  async function handleStart() {
    if (!selectedCharacterId || isBusy()) return

    isStarting = true
    errorMessage = ''
    const result = await onStartGame(selectedCharacterId)
    isStarting = false

    if (!result.ok) {
      errorMessage = result.message ?? 'Failed to enter game'
    }
  }

  async function handleDelete() {
    if (!selectedCharacterId || isBusy()) return

    const character = characters.find((c) => c.id === selectedCharacterId)
    if (!character) return

    const confirmed = confirm(
      `Are you sure you want to delete "${character.name}"? This cannot be undone.`
    )
    if (!confirmed) return

    isDeleting = true
    errorMessage = ''
    const result = await onDeleteCharacter(selectedCharacterId)
    isDeleting = false

    if (!result.ok) {
      errorMessage = result.message ?? 'Failed to delete character'
    }
  }
</script>

<div class="character-select-screen">
  <!-- 3D Canvas Layer -->
  <div class="canvas-layer">
    <Canvas shadows>
      <CharacterSelectScene {characters} {selectedCharacterId} />
    </Canvas>
  </div>

  <!-- HTML Overlay Layer -->
  <div class="overlay-layer">
    <div class="top-bar">
      <h1 class="title">Character Select</h1>
      <p class="account-name">Account: {accountName}</p>
    </div>

    {#if viewMode === 'select'}
      <div class="character-columns">
        {#each [0, 1, 2] as slotIndex (slotIndex)}
          {@const character = characters[slotIndex]}
          <button
            type="button"
            class="character-column"
            class:selected={character?.id === selectedCharacterId}
            class:empty={!character}
            onclick={() => handleSlotClick(slotIndex)}
            disabled={isBusy()}
          >
            {#if character}
              <div class="char-name">{character.name}</div>
              <div class="char-stats">
                <span class="stat">STR {character.attributes.str}</span>
                <span class="stat">DEX {character.attributes.dex}</span>
                <span class="stat">CON {character.attributes.con}</span>
                <span class="stat">INT {character.attributes.int}</span>
                <span class="stat">WIS {character.attributes.wis}</span>
                <span class="stat">CHA {character.attributes.cha}</span>
              </div>
            {:else}
              <div class="empty-slot">+ Create</div>
            {/if}
          </button>
        {/each}
      </div>

      <div class="bottom-bar">
        {#if errorMessage}
          <div class="error-message">{errorMessage}</div>
        {/if}
        <div class="actions">
          <button
            type="button"
            class="primary"
            onclick={handleStart}
            disabled={!selectedCharacterId || isBusy()}
          >
            {isStarting ? 'Starting...' : 'Start'}
          </button>
          <button
            type="button"
            class="danger"
            onclick={handleDelete}
            disabled={!selectedCharacterId || isBusy()}
          >
            {isDeleting ? 'Deleting...' : 'Delete'}
          </button>
          <button
            type="button"
            class="secondary"
            onclick={onLogout}
            disabled={isBusy()}
          >
            Back
          </button>
        </div>
      </div>
    {/if}
  </div>

  <!-- Create Mode Modal Overlay -->
  {#if viewMode === 'create'}
    <div class="create-overlay">
      <div class="create-panel">
        <h2 class="create-title">Create Character</h2>
        <form class="create-form" onsubmit={submitCreateCharacter}>
          <label for="characterName">Character Name</label>
          <input
            id="characterName"
            type="text"
            bind:value={createCharacterName}
            maxlength={24}
            placeholder="Enter character name"
            disabled={isBusy()}
          />

          <div class="rolled-attributes">
            {#if rolledAttributes}
              <div class="attr">STR {rolledAttributes.str}</div>
              <div class="attr">DEX {rolledAttributes.dex}</div>
              <div class="attr">CON {rolledAttributes.con}</div>
              <div class="attr">INT {rolledAttributes.int}</div>
              <div class="attr">WIS {rolledAttributes.wis}</div>
              <div class="attr">CHA {rolledAttributes.cha}</div>
            {:else}
              <div class="roll-hint">Roll to generate attributes (4d6 drop lowest, total 72)</div>
            {/if}
          </div>

          {#if errorMessage}
            <div class="error-message">{errorMessage}</div>
          {/if}

          <div class="create-actions">
            <button type="button" class="secondary" disabled={isBusy()} onclick={handleRoll}>
              {isRolling ? 'Rolling...' : 'Roll'}
            </button>
            <button
              type="submit"
              class="primary"
              disabled={isBusy() || !rolledAttributes}
            >
              {isCreating ? 'Creating...' : 'Create'}
            </button>
            <button
              type="button"
              class="secondary"
              disabled={isBusy()}
              onclick={() => {
                viewMode = 'select'
                rolledAttributes = null
                errorMessage = ''
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      </div>
    </div>
  {/if}
</div>

<style>
  .character-select-screen {
    position: fixed;
    inset: 0;
    background: linear-gradient(140deg, #0f1621 0%, #1e2d43 55%, #263a58 100%);
  }

  .canvas-layer {
    position: absolute;
    inset: 0;
    z-index: 0;
  }

  .overlay-layer {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    pointer-events: none;
    color: #edf2f7;
  }

  .top-bar {
    text-align: center;
    padding: 32px 16px 0;
  }

  .title {
    margin: 0;
    font-size: 28px;
    text-shadow: 0 2px 8px rgba(0, 0, 0, 0.6);
  }

  .account-name {
    margin: 6px 0 0;
    color: #9fb0c6;
    font-size: 13px;
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
  }

  .character-columns {
    display: flex;
    justify-content: center;
    gap: 24px;
    padding: 0 10%;
    margin-top: auto;
    padding-bottom: 16px;
  }

  .character-column {
    flex: 1;
    max-width: 200px;
    min-height: 100px;
    border-radius: 10px;
    border: 1px solid rgba(83, 101, 123, 0.5);
    background: rgba(20, 30, 44, 0.6);
    color: #f7fafc;
    text-align: center;
    padding: 14px 12px;
    pointer-events: auto;
    cursor: pointer;
    backdrop-filter: blur(4px);
    transition:
      border-color 0.18s,
      background-color 0.18s;
  }

  .character-column:hover:not(:disabled) {
    border-color: #6fa3ff;
    background: rgba(26, 41, 64, 0.7);
  }

  .character-column.selected {
    border-color: #7cc9ff;
    background: rgba(34, 53, 82, 0.7);
  }

  .character-column.empty {
    color: #9fb0c6;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .character-column:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .char-name {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 8px;
  }

  .char-stats {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 3px 8px;
  }

  .stat {
    font-size: 11px;
    color: #a7b7ca;
  }

  .empty-slot {
    font-size: 14px;
    font-weight: 500;
  }

  .bottom-bar {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 0 16px 32px;
    pointer-events: auto;
  }

  .actions {
    display: flex;
    gap: 10px;
  }

  .actions button {
    min-width: 100px;
    border-radius: 7px;
    padding: 10px 20px;
    font-size: 14px;
    cursor: pointer;
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* Create overlay */
  .create-overlay {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(2px);
  }

  .create-panel {
    width: min(420px, calc(100vw - 32px));
    border-radius: 12px;
    background: rgba(6, 10, 16, 0.92);
    border: 1px solid #45556b;
    box-shadow: 0 16px 38px rgba(0, 0, 0, 0.45);
    padding: 24px;
    color: #edf2f7;
  }

  .create-title {
    margin: 0 0 16px;
    font-size: 20px;
    text-align: center;
  }

  .create-form {
    display: grid;
    gap: 10px;
  }

  .create-form label {
    font-size: 13px;
    color: #b8c6d9;
  }

  .create-form input {
    border: 1px solid #526276;
    border-radius: 7px;
    padding: 10px 12px;
    background: #111923;
    color: #f7fafc;
    font-size: 14px;
  }

  .rolled-attributes {
    border: 1px solid #45556b;
    border-radius: 8px;
    background: rgba(16, 24, 35, 0.9);
    padding: 10px;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
    min-height: 54px;
    align-items: center;
  }

  .attr {
    font-size: 13px;
    font-weight: 600;
    color: #e4ecf5;
    text-align: center;
  }

  .roll-hint {
    grid-column: 1 / -1;
    font-size: 12px;
    color: #9fb0c6;
    text-align: center;
  }

  .create-actions {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 10px;
  }

  .create-actions button {
    border-radius: 7px;
    padding: 10px 12px;
    font-size: 14px;
    cursor: pointer;
  }

  .create-actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* Shared button styles */
  .primary {
    border: none;
    background: #2c7be5;
    color: white;
    font-weight: 600;
  }

  .secondary {
    border: 1px solid #61738a;
    background: #1c2736;
    color: #dbe6f2;
  }

  .danger {
    border: 1px solid #b04040;
    background: #3a1a1a;
    color: #ffa0a0;
  }

  .error-message {
    border: 1px solid #f28b8b;
    border-radius: 7px;
    padding: 10px 12px;
    background: rgba(175, 45, 45, 0.2);
    color: #ffd2d2;
    font-size: 13px;
    max-width: 400px;
    text-align: center;
  }
</style>
