<script lang="ts">
  import { Canvas } from '@threlte/core'
  import type { AccountCharacter, CharacterAttributes } from '../network/socket'
  import CharacterCreateScene from './CharacterCreateScene.svelte'

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
    onCharacterCreated: (characterId: number) => void
    onCancel: () => void
  }

  let {
    accountName,
    characters,
    onRollCharacterStats,
    onCreateCharacter,
    onCharacterCreated,
    onCancel,
  }: Props = $props()

  let createCharacterName = $state('')
  let rolledAttributes = $state<CharacterAttributes | null>(null)
  let isCreating = $state(false)
  let isRolling = $state(false)
  let errorMessage = $state('')

  function isBusy() {
    return isCreating || isRolling
  }

  function atSlotLimit() {
    return characters.length >= MAX_CHARACTER_SLOTS
  }

  async function handleRoll() {
    if (isBusy()) return
    if (atSlotLimit()) {
      errorMessage = 'A maximum of 3 characters can be created.'
      return
    }

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

    if (atSlotLimit()) {
      errorMessage = 'A maximum of 3 characters can be created.'
      return
    }

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

    if (!result.character) {
      errorMessage = 'Character created but no character data returned'
      return
    }

    createCharacterName = ''
    rolledAttributes = null
    onCharacterCreated(result.character.id)
  }
</script>

<div class="character-create-screen">
  <div class="canvas-layer">
    <Canvas shadows>
      <CharacterCreateScene />
    </Canvas>
  </div>

  <div class="overlay-layer">
    <div class="top-bar">
      <h1 class="title">Create Character</h1>
      <p class="account-name">Account: {accountName}</p>
    </div>

    <div class="bottom-bar">
      {#if errorMessage}
        <div class="error-message">{errorMessage}</div>
      {/if}

      <form class="create-form" onsubmit={submitCreateCharacter}>
        <label class="name-field" for="characterName">
          <span>Name</span>
          <input
            id="characterName"
            type="text"
            bind:value={createCharacterName}
            maxlength={24}
            placeholder="Enter character name"
            disabled={isBusy()}
          />
        </label>

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
            disabled={isBusy() || !rolledAttributes || atSlotLimit()}
          >
            {isCreating ? 'Creating...' : 'Create'}
          </button>
          <button
            type="button"
            class="secondary"
            disabled={isBusy()}
            onclick={onCancel}
          >
            Cancel
          </button>
        </div>
      </form>
    </div>
  </div>
</div>

<style>
  .character-create-screen {
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
    color: #edf2f7;
    pointer-events: none;
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

  .bottom-bar {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 16px 24px;
    pointer-events: auto;
  }

  .create-form {
    display: flex;
    align-items: stretch;
    gap: 10px;
    padding: 12px;
    border-radius: 12px;
    border: 1px solid #45556b;
    background: rgba(6, 10, 16, 0.9);
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.35);
  }

  .name-field {
    min-width: 220px;
    max-width: 320px;
    flex: 1;
    display: grid;
    gap: 6px;
  }

  .name-field span {
    font-size: 13px;
    color: #b8c6d9;
  }

  .name-field input {
    border: 1px solid #526276;
    border-radius: 7px;
    padding: 10px 12px;
    background: #111923;
    color: #f7fafc;
    font-size: 14px;
  }

  .rolled-attributes {
    width: 320px;
    max-width: 100%;
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
    width: 330px;
    max-width: 100%;
    display: flex;
    align-items: end;
    gap: 10px;
    margin-left: auto;
  }

  .create-actions button {
    flex: 1;
    border-radius: 7px;
    height: 34px;
    padding: 6px 12px;
    font-size: 14px;
    line-height: 1.2;
    cursor: pointer;
  }

  .create-actions button:disabled {
    opacity: 0.5;
    cursor: default;
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
    text-align: center;
  }

  @media (max-width: 1100px) {
    .create-form {
      flex-direction: column;
    }

    .name-field,
    .rolled-attributes,
    .create-actions {
      width: 100%;
      max-width: none;
    }

    .create-actions {
      margin-left: 0;
    }
  }
</style>
