<script lang="ts">
  import type { AccountCharacter } from '../network/socket'

  interface Props {
    accountName: string
    characters: AccountCharacter[]
    selectedCharacterId: number | null
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

<!-- UI overlay only — the 3D scene is rendered in the shared Canvas in App.svelte -->
<div class="character-select-overlay">
  <div class="top-bar">
    <h1 class="title">Character Select</h1>
    <p class="account-name">Account: {accountName}</p>
  </div>

  <div class="bottom-row">
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
    {#if errorMessage}
      <div class="error-message">{errorMessage}</div>
    {/if}
  </div>
</div>

<style>
  .character-select-overlay {
    position: fixed;
    inset: 0;
    z-index: 1;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    pointer-events: none;
    color: #edf2f7;
    /* No background — the gradient is rendered behind the shared Canvas in App.svelte */
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

  .bottom-row {
    position: fixed;
    bottom: 16px;
    left: 16px;
    right: 60px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    pointer-events: auto;
  }

  .bottom-row button {
    border-radius: 7px;
    padding: 8px 16px;
    font-size: 14px;
    cursor: pointer;
  }

  .bottom-row button:disabled {
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
    position: absolute;
    bottom: 100%;
    left: 50%;
    transform: translateX(-50%);
    margin-bottom: 10px;
    border: 1px solid #f28b8b;
    border-radius: 7px;
    padding: 10px 12px;
    background: rgba(175, 45, 45, 0.2);
    color: #ffd2d2;
    font-size: 13px;
    max-width: 400px;
    text-align: center;
    white-space: nowrap;
  }
</style>
