<script lang="ts">
  import { Canvas } from '@threlte/core'
  import type { AccountCharacter } from '../network/socket'
  import CharacterSelectScene from './CharacterSelectScene.svelte'
  import { createWebGPURenderer } from '../utils/renderer'

  const MAX_CHARACTER_SLOTS = 3

  interface Props {
    accountName: string
    characters: AccountCharacter[]
    selectedCharacterId: number | null
    onSelectCharacter: (characterId: number) => void
    onRequestCreateCharacter: () => void
    onStartGame: (characterId: number) => Promise<{ ok: boolean; message?: string }>
    onDeleteCharacter: (
      characterId: number
    ) => Promise<{ ok: boolean; message?: string }>
    onLogout: () => void
  }

  let {
    accountName,
    characters,
    selectedCharacterId,
    onSelectCharacter,
    onRequestCreateCharacter,
    onStartGame,
    onDeleteCharacter,
    onLogout,
  }: Props = $props()

  let isStarting = $state(false)
  let isDeleting = $state(false)
  let errorMessage = $state('')

  function isBusy() {
    return isStarting || isDeleting
  }

  function handleSlotClick(slotIndex: number) {
    if (isBusy()) return

    const character = characters[slotIndex]
    errorMessage = ''
    if (character) {
      onSelectCharacter(character.id)
      return
    }

    if (characters.length >= MAX_CHARACTER_SLOTS) {
      errorMessage = 'A maximum of 3 characters can be created.'
      return
    }

    onRequestCreateCharacter()
  }

  async function handleSlotDoubleClick(slotIndex: number) {
    if (isBusy()) return
    const character = characters[slotIndex]
    if (!character) return
    onSelectCharacter(character.id)
    await handleStart(character.id)
  }

  async function handleStart(characterId?: number) {
    const id = characterId ?? selectedCharacterId
    if (!id || isBusy()) return

    isStarting = true
    errorMessage = ''
    const result = await onStartGame(id)
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
  <div class="canvas-layer">
    <Canvas renderMode="always" shadows createRenderer={createWebGPURenderer}>
      <CharacterSelectScene
        {characters}
        {selectedCharacterId}
        onSlotClick={handleSlotClick}
        onSlotDoubleClick={handleSlotDoubleClick}
      />
    </Canvas>
  </div>

  <div class="overlay-layer">
    <div class="top-bar">
      <h1 class="title">Character Select</h1>
      <p class="account-name">Account: {accountName}</p>
    </div>

    <div class="bottom-bar">
      {#if errorMessage}
        <div class="error-message">{errorMessage}</div>
      {/if}
      <div class="actions">
        <button
          type="button"
          class="secondary"
          onclick={onLogout}
          disabled={isBusy()}
        >
          Back
        </button>
        <button
          type="button"
          class="primary"
          onclick={() => handleStart()}
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
      </div>
    </div>
  </div>
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
    justify-content: space-between;
    width: 100%;
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
