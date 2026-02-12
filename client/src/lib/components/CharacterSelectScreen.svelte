<script lang="ts">
  import type { AccountCharacter } from '../network/socket'

  const MAX_CHARACTER_SLOTS = 3

  interface Props {
    accountName: string
    characters: AccountCharacter[]
    onCreateCharacter: (
      characterName: string
    ) => Promise<{ ok: boolean; message?: string; character?: AccountCharacter }>
    onStartGame: (characterId: number) => Promise<{ ok: boolean; message?: string }>
    onLogout: () => void
  }

  let { accountName, characters, onCreateCharacter, onStartGame, onLogout }: Props =
    $props()

  let selectedCharacterId = $state<number | null>(null)
  let createCharacterName = $state('')
  let viewMode = $state<'select' | 'create'>('select')
  let isSubmitting = $state(false)
  let errorMessage = $state('')

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
    if (isSubmitting) return

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
    viewMode = 'create'
  }

  async function submitCreateCharacter(event: Event) {
    event.preventDefault()
    if (isSubmitting) return

    const characterName = createCharacterName.trim()
    if (!characterName) {
      errorMessage = 'Please enter character name'
      return
    }

    isSubmitting = true
    errorMessage = ''
    const result = await onCreateCharacter(characterName)
    isSubmitting = false

    if (!result.ok) {
      errorMessage = result.message ?? 'Failed to create character'
      return
    }

    if (result.character) {
      selectedCharacterId = result.character.id
    }
    createCharacterName = ''
    viewMode = 'select'
  }

  async function handleStart() {
    if (!selectedCharacterId || isSubmitting) return

    isSubmitting = true
    errorMessage = ''
    const result = await onStartGame(selectedCharacterId)
    isSubmitting = false

    if (!result.ok) {
      errorMessage = result.message ?? 'Failed to enter game'
    }
  }
</script>

<div class="character-select-container">
  <div class="character-select-panel">
    <h1 class="title">Character Select</h1>
    <p class="account-name">Account: {accountName}</p>

    {#if viewMode === 'select'}
      <div class="slots">
        {#each [0, 1, 2] as slotIndex (slotIndex)}
          {@const character = characters[slotIndex]}
          <button
            type="button"
            class="slot"
            class:selected={character?.id === selectedCharacterId}
            class:empty={!character}
            onclick={() => handleSlotClick(slotIndex)}
            disabled={isSubmitting}
          >
            {#if character}
              <div class="slot-name">{character.name}</div>
            {:else}
              <div class="slot-empty">+ Create Character</div>
            {/if}
          </button>
        {/each}
      </div>
    {:else}
      <form class="create-form" onsubmit={submitCreateCharacter}>
        <label for="characterName">Character Name</label>
        <input
          id="characterName"
          type="text"
          bind:value={createCharacterName}
          maxlength={24}
          placeholder="Enter character name"
          disabled={isSubmitting}
        />
        <div class="create-actions">
          <button type="submit" class="primary" disabled={isSubmitting}>
            {isSubmitting ? 'Creating...' : 'Create'}
          </button>
          <button
            type="button"
            class="secondary"
            disabled={isSubmitting}
            onclick={() => {
              viewMode = 'select'
              errorMessage = ''
            }}
          >
            Cancel
          </button>
        </div>
      </form>
    {/if}

    {#if errorMessage}
      <div class="error-message">{errorMessage}</div>
    {/if}

    {#if viewMode === 'select'}
      <div class="actions">
        <button
          type="button"
          class="primary"
          onclick={handleStart}
          disabled={!selectedCharacterId || isSubmitting}
        >
          {isSubmitting ? 'Starting...' : 'Start'}
        </button>
        <button
          type="button"
          class="secondary"
          onclick={onLogout}
          disabled={isSubmitting}
        >
          Back
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .character-select-container {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(140deg, #0f1621 0%, #1e2d43 55%, #263a58 100%);
  }

  .character-select-panel {
    width: min(460px, calc(100vw - 32px));
    border-radius: 12px;
    background: rgba(6, 10, 16, 0.88);
    border: 1px solid #45556b;
    box-shadow: 0 16px 38px rgba(0, 0, 0, 0.45);
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    color: #edf2f7;
  }

  .title {
    margin: 0;
    font-size: 26px;
    text-align: center;
  }

  .account-name {
    margin: 0;
    text-align: center;
    color: #9fb0c6;
    font-size: 13px;
  }

  .slots {
    display: grid;
    gap: 10px;
  }

  .slot {
    width: 100%;
    min-height: 66px;
    border-radius: 8px;
    border: 1px solid #53657b;
    background: #141e2c;
    color: #f7fafc;
    text-align: left;
    padding: 12px 14px;
    transition:
      border-color 0.18s,
      background-color 0.18s;
  }

  .slot:hover:not(:disabled) {
    border-color: #6fa3ff;
    background: #1a2940;
  }

  .slot.selected {
    border-color: #7cc9ff;
    background: #223552;
  }

  .slot.empty {
    color: #9fb0c6;
  }

  .slot-name {
    font-size: 16px;
    font-weight: 600;
  }

  .slot-empty {
    font-size: 14px;
    font-weight: 500;
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

  .create-actions,
  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

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

  .error-message {
    border: 1px solid #f28b8b;
    border-radius: 7px;
    padding: 10px 12px;
    background: rgba(175, 45, 45, 0.2);
    color: #ffd2d2;
    font-size: 13px;
  }
</style>
