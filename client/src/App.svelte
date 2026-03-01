<script lang="ts">
  import { Canvas } from '@threlte/core'
  import GameScene from './lib/components/GameScene.svelte'
  import ChatPanel from './lib/components/ChatPanel.svelte'
  import FPSCounter from './lib/components/FPSCounter.svelte'
  import GameTimeWidget from './lib/components/GameTimeWidget.svelte'
  import CelestialDebugDialog from './lib/components/CelestialDebugDialog.svelte'
  import LoginScreen from './lib/components/LoginScreen.svelte'
  import CharacterSelectScreen from './lib/components/CharacterSelectScreen.svelte'
  import CharacterCreateScreen from './lib/components/CharacterCreateScreen.svelte'
  import RespawnDialog from './lib/components/RespawnDialog.svelte'
  import LoadingDialog from './lib/components/LoadingDialog.svelte'
  import WorldMapDialog from './lib/components/WorldMapDialog.svelte'
  import CharacterAttributesHud from './lib/components/CharacterAttributesHud.svelte'
  import { gameStore } from './lib/stores/gameStore'
  import { mapEditorMode, worldMapVisible, teleportLoading } from './lib/stores/debugStore'
  import { hoveredCell, editorTool } from './lib/stores/editorStore'
  import HeightBrushPanel from './lib/components/map-editor/HeightBrushPanel.svelte'
  import SplatBrushPanel from './lib/components/map-editor/SplatBrushPanel.svelte'
  import { networkManager, type AccountCharacter, type CharacterClass } from './lib/network/socket'

  type AppScreen = 'login' | 'character-select' | 'character-create' | 'game'
  type DeathUiState = 'alive' | 'waiting_dying' | 'dialog_open' | 'dialog_closed'
  let screen = $state<AppScreen>('login')
  let serverUrl = $state('')
  let accountName = $state('')
  let accountCharacters = $state<AccountCharacter[]>([])
  let selectedCharacterId = $state<number | null>(null)
  let selectedCharacter = $derived<AccountCharacter | null>(
    accountCharacters.find((character) => character.id === selectedCharacterId) ?? null
  )
  let isPlayerDead = $state(false)
  let currentPlayerHp = $state<number | null>(null)
  let currentPlayerMaxHp = $state<number | null>(null)
  let currentPlayerLevel = $state<number | null>(null)
  let currentPlayerTotalXp = $state<number | null>(null)
  let deathUiState = $state<DeathUiState>('alive')
  let showRespawnDialog = $derived(deathUiState === 'dialog_open')
  let canReopenRespawnDialog = $derived(
    isPlayerDead && deathUiState === 'dialog_closed'
  )
  let wasPlayerDead = false
  let isCurrentPlayerLoading = $state(false)
  let kickedMessage = $state('')

  $effect(() => {
    if (selectedCharacterId === null) {
      if (accountCharacters.length > 0) {
        selectedCharacterId = accountCharacters[0].id
      }
      return
    }

    const selectedStillExists = accountCharacters.some(
      (character) => character.id === selectedCharacterId
    )
    if (!selectedStillExists) {
      selectedCharacterId = accountCharacters.length > 0 ? accountCharacters[0].id : null
    }
  })

  async function handleLogin(
    url: string,
    account: string,
    pass: string,
    createAccount: boolean
  ): Promise<{ ok: boolean; message?: string }> {
    kickedMessage = ''
    const result = await networkManager.requestAuthentication(
      url,
      account,
      pass,
      createAccount
    )

    if (result.ok) {
      const characters = result.characters ?? []
      serverUrl = url
      accountName = result.accountName ?? account
      accountCharacters = characters
      selectedCharacterId = characters.length > 0 ? characters[0].id : null
      screen = 'character-select'
      return { ok: true }
    }

    return result
  }

  async function handleCreateCharacter(characterName: string, characterClass: CharacterClass) {
    const result = await networkManager.requestCreateCharacter(characterName, characterClass)
    if (result.ok && result.character) {
      accountCharacters = [...accountCharacters, result.character]
    }
    return result
  }

  async function handleDeleteCharacter(characterId: number) {
    const result = await networkManager.requestDeleteCharacter(characterId)
    if (result.ok) {
      accountCharacters = accountCharacters.filter((c) => c.id !== characterId)
    }
    return result
  }

  async function handleRollCharacterStats() {
    return networkManager.requestRollCharacterStats()
  }

  async function handleStartGame(
    characterId: number
  ): Promise<{ ok: boolean; message?: string }> {
    const result = await networkManager.requestEnterGame(characterId)
    if (result.ok) {
      screen = 'game'
    }
    return result
  }

  function handleOpenCreateCharacterScreen() {
    if (accountCharacters.length >= 3) return
    screen = 'character-create'
  }

  function handleCancelCreateCharacter() {
    screen = 'character-select'
  }

  function handleCharacterCreated(characterId: number) {
    selectedCharacterId = characterId
    screen = 'character-select'
  }

  function handleSelectCharacter(characterId: number) {
    selectedCharacterId = characterId
  }

  async function handleBackToCharacterSelect() {
    screen = 'character-select'
    const result = await networkManager.requestReauthenticate()
    if (result.ok) {
      accountCharacters = result.characters ?? []
      if (result.accountName) accountName = result.accountName
      if (accountCharacters.length > 0) {
        const stillExists = accountCharacters.some(
          (c) => c.id === selectedCharacterId
        )
        if (!stillExists) {
          selectedCharacterId = accountCharacters[0].id
        }
      } else {
        selectedCharacterId = null
      }
    } else {
      handleLogoutToLogin()
    }
  }

  function handleLogoutToLogin() {
    networkManager.disconnect()
    accountName = ''
    accountCharacters = []
    selectedCharacterId = null
    screen = 'login'
  }

  function requestRespawn() {
    deathUiState = 'dialog_closed'
    networkManager.requestRespawn()
  }

  function closeRespawnDialog() {
    deathUiState = isPlayerDead ? 'dialog_closed' : 'alive'
  }

  function reopenRespawnDialog() {
    if (!isPlayerDead || deathUiState !== 'dialog_closed') return
    deathUiState = 'dialog_open'
  }

  function handleCurrentPlayerDyingFinished() {
    if (screen !== 'game' || !isPlayerDead || deathUiState !== 'waiting_dying') return
    deathUiState = 'dialog_open'
  }

  networkManager.kicked.on((reason) => {
    kickedMessage = reason
    accountName = ''
    accountCharacters = []
    selectedCharacterId = null
    deathUiState = 'alive'
    isPlayerDead = false
    screen = 'login'
  })

  gameStore.subscribe((state) => {
    currentPlayerHp = state.currentPlayer?.health ?? null
    currentPlayerMaxHp = state.currentPlayer?.maxHealth ?? null
    currentPlayerLevel = state.currentPlayer?.level ?? null
    currentPlayerTotalXp = state.currentPlayer?.totalXp ?? null
    const deadNow =
      screen === 'game' &&
      !!state.currentPlayer &&
      state.currentPlayer.health <= 0
    if (deadNow && !wasPlayerDead) {
      deathUiState = 'waiting_dying'
    }
    if (!deadNow) {
      deathUiState = 'alive'
    }
    isPlayerDead = deadNow
    wasPlayerDead = deadNow
  })
</script>

<main>
  {#if screen === 'game'}
    <div class="game-shell" class:dead={isPlayerDead}>
      <Canvas renderMode="always" shadows>
        <GameScene
          {serverUrl}
          onCurrentPlayerDyingFinished={handleCurrentPlayerDyingFinished}
          bind:isCurrentPlayerLoading
        />
      </Canvas>
      <ChatPanel />
      <FPSCounter />
      <GameTimeWidget />
      <CelestialDebugDialog />
      {#if $mapEditorMode}
        <div class="editor-mode-badge">
          MAP EDITOR{#if $hoveredCell}
            <span class="cell-info">
              T({$hoveredCell.tileX}, {$hoveredCell.tileZ})
              C({$hoveredCell.cellX}, {$hoveredCell.cellZ})
            </span>
          {/if}
        </div>
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
        </div>
        {#if $editorTool === 'height'}
          <HeightBrushPanel />
        {:else}
          <SplatBrushPanel />
        {/if}
      {/if}
      {#if selectedCharacter}
        <CharacterAttributesHud
          level={currentPlayerLevel ?? selectedCharacter.level}
          currentXp={currentPlayerTotalXp ?? selectedCharacter.xp}
          currentHp={currentPlayerHp ?? selectedCharacter.max_hp}
          maxHp={currentPlayerMaxHp ?? selectedCharacter.max_hp}
          attributes={selectedCharacter.attributes}
        />
      {/if}

      <div class="corner-actions">
        {#if canReopenRespawnDialog}
          <button class="respawn-reopen" onclick={reopenRespawnDialog}>
            Respawn
          </button>
        {/if}
        <button class="back-to-select" onclick={handleBackToCharacterSelect}>
          Character Select
        </button>
      </div>
    </div>

    {#if isCurrentPlayerLoading || $teleportLoading}
      <LoadingDialog />
    {/if}

    {#if showRespawnDialog}
      <RespawnDialog onRespawn={requestRespawn} onLater={closeRespawnDialog} />
    {/if}

    {#if $worldMapVisible}
      <WorldMapDialog />
    {/if}
  {:else if screen === 'character-select'}
    <CharacterSelectScreen
      {accountName}
      characters={accountCharacters}
      {selectedCharacterId}
      onSelectCharacter={handleSelectCharacter}
      onRequestCreateCharacter={handleOpenCreateCharacterScreen}
      onStartGame={handleStartGame}
      onDeleteCharacter={handleDeleteCharacter}
      onLogout={handleLogoutToLogin}
    />
  {:else if screen === 'character-create'}
    <CharacterCreateScreen
      {accountName}
      characters={accountCharacters}
      onRollCharacterStats={handleRollCharacterStats}
      onCreateCharacter={handleCreateCharacter}
      onCharacterCreated={handleCharacterCreated}
      onCancel={handleCancelCreateCharacter}
    />
  {:else}
    <LoginScreen onLogin={handleLogin} {kickedMessage} />
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
    background: #1a1a1a;
  }

  main {
    width: 100vw;
    height: 100vh;
    position: relative;
  }

  .game-shell {
    width: 100%;
    height: 100%;
    transition: filter 180ms ease;
  }

  .game-shell.dead {
    filter: grayscale(100%);
  }

  .corner-actions {
    position: absolute;
    right: 16px;
    bottom: 16px;
    z-index: 30;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
  }

  .respawn-reopen,
  .back-to-select {
    border: none;
    border-radius: 8px;
    padding: 10px 14px;
    font-size: 13px;
    cursor: pointer;
  }

  .respawn-reopen {
    background: #e2b93b;
    color: #1a1a1a;
    font-weight: 700;
  }

  .back-to-select {
    background: rgba(60, 60, 60, 0.85);
    color: #ccc;
    font-weight: 600;
    transition: background 150ms ease, color 150ms ease;
  }

  .back-to-select:hover {
    background: rgba(80, 80, 80, 0.95);
    color: #fff;
  }

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

  .editor-tool-tabs {
    position: fixed;
    left: 16px;
    bottom: 180px;
    z-index: 1000;
    display: flex;
    gap: 2px;
    background: rgba(0, 0, 0, 0.7);
    border-radius: 6px;
    padding: 3px;
    border: 1px solid rgba(226, 185, 59, 0.3);
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

</style>
