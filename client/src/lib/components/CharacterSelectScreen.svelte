<script lang="ts">
  import type { AccountCharacter, CharacterAttributes } from '../network/socket'

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
    onLogout: () => void
  }

  let {
    accountName,
    characters,
    onRollCharacterStats,
    onCreateCharacter,
    onStartGame,
    onLogout,
  }: Props = $props()

  let selectedCharacterId = $state<number | null>(null)
  let createCharacterName = $state('')
  let rolledAttributes = $state<CharacterAttributes | null>(null)
  let viewMode = $state<'select' | 'create'>('select')
  let isCreating = $state(false)
  let isRolling = $state(false)
  let isStarting = $state(false)
  let errorMessage = $state('')

  function isBusy() {
    return isCreating || isRolling || isStarting
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
            disabled={isBusy()}
          >
            {#if character}
              <div class="slot-name">{character.name}</div>
              <div class="slot-stats">
                STR {character.attributes.str} DEX {character.attributes.dex} CON {character.attributes.con} INT {character.attributes.int} WIS {character.attributes.wis} CHA {character.attributes.cha}
              </div>
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
          disabled={!selectedCharacterId || isBusy()}
        >
          {isStarting ? 'Starting...' : 'Start'}
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

  .slot-stats {
    margin-top: 6px;
    font-size: 12px;
    color: #a7b7ca;
    line-height: 1.4;
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

  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .create-actions button,
  .actions button {
    border-radius: 7px;
    padding: 10px 12px;
    font-size: 14px;
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
