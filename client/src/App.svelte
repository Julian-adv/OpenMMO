<script lang="ts">
  import { Canvas } from '@threlte/core'
  import GameScene from './lib/components/GameScene.svelte'
  import ChatPanel from './lib/components/ChatPanel.svelte'
  import FPSCounter from './lib/components/FPSCounter.svelte'
  import GameTimeWidget from './lib/components/GameTimeWidget.svelte'
  import CelestialDebugDialog from './lib/components/CelestialDebugDialog.svelte'
  import LoginScreen from './lib/components/LoginScreen.svelte'
  import CharacterSelectScreen from './lib/components/CharacterSelectScreen.svelte'
  import CharacterSelectScene from './lib/components/CharacterSelectScene.svelte'
  import CharacterCreateScreen from './lib/components/CharacterCreateScreen.svelte'
  import CharacterCreateScene from './lib/components/CharacterCreateScene.svelte'
  import RespawnDialog from './lib/components/RespawnDialog.svelte'
  import LoadingDialog from './lib/components/LoadingDialog.svelte'
  import WorldMapDialog from './lib/components/WorldMapDialog.svelte'
  import CharacterAttributesHud from './lib/components/CharacterAttributesHud.svelte'
  import { gameStore } from './lib/stores/gameStore'
  import { mapEditorMode, worldMapVisible, teleportLoading } from './lib/stores/debugStore'
  import { createWebGPURenderer } from './lib/utils/renderer'
  import MapEditorPanel from './lib/components/map-editor/MapEditorPanel.svelte'
  import GenerateTerrainDialog from './lib/components/map-editor/GenerateTerrainDialog.svelte'
  import { showGenerateDialog } from './lib/stores/editorStore'
  import { networkManager, type AccountCharacter, type CharacterClass } from './lib/network/socket'
  import { startBgm } from './lib/managers/bgmManager'
  import SettingsPanel from './lib/components/SettingsPanel.svelte'

  let showSettings = $state(false)

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
  let isSceneCompiling = $state(true)
  let kickedMessage = $state('')

  // Character create screen state
  let createSelectedClass = $state<CharacterClass>('warrior')

  // Whether the shared Canvas should be mounted (all screens except login)
  let showCanvas = $derived(screen !== 'login')

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
      isSceneCompiling = true
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

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<main onclick={startBgm}>
  <!-- Shared Canvas: one WebGPU device across character select, create, and game.
       Pipelines compiled during character select are reused in game. -->
  {#if showCanvas}
    <div class="canvas-layer" class:dead={screen === 'game' && isPlayerDead}>
      <Canvas renderMode="always" shadows createRenderer={createWebGPURenderer}>
        {#if screen === 'character-select'}
          <CharacterSelectScene
            characters={accountCharacters}
            {selectedCharacterId}
            onSlotClick={(i) => {
              const c = accountCharacters[i]
              if (c) { handleSelectCharacter(c.id) } else { handleOpenCreateCharacterScreen() }
            }}
            onSlotDoubleClick={(i) => {
              const c = accountCharacters[i]
              if (c) { handleSelectCharacter(c.id); handleStartGame(c.id) }
            }}
          />
        {:else if screen === 'character-create'}
          <CharacterCreateScene characterClass={createSelectedClass} />
        {:else if screen === 'game'}
          <GameScene
            {serverUrl}
            onCurrentPlayerDyingFinished={handleCurrentPlayerDyingFinished}
            bind:isCurrentPlayerLoading
            bind:isSceneCompiling
          />
        {/if}
      </Canvas>
    </div>
  {/if}

  <!-- UI overlays (outside Canvas) -->
  {#if screen === 'game'}
    <div class="game-hud">
      <ChatPanel />
      <FPSCounter />
      <GameTimeWidget />
      <CelestialDebugDialog />
      {#if $mapEditorMode}
        <MapEditorPanel />
      {/if}
      {#if $showGenerateDialog}
        <GenerateTerrainDialog />
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
        <button class="back-to-select" onclick={handleBackToCharacterSelect} title="Character Select">
          <svg xmlns="http://www.w3.org/2000/svg" width="640" height="512" viewBox="0 0 640 512"><path fill="currentColor" d="M72 88a56 56 0 1 1 112 0a56 56 0 1 1-112 0m-8 157.7c-10 11.2-16 26.1-16 42.3s6 31.1 16 42.3v-84.7zm144.4-49.3C178.7 222.7 160 261.2 160 304c0 34.3 12 65.8 32 90.5V416c0 17.7-14.3 32-32 32H96c-17.7 0-32-14.3-32-32v-26.8C26.2 371.2 0 332.7 0 288c0-61.9 50.1-112 112-112h32c24 0 46.2 7.5 64.4 20.3zM448 416v-21.5c20-24.7 32-56.2 32-90.5c0-42.8-18.7-81.3-48.4-107.7C449.8 183.5 472 176 496 176h32c61.9 0 112 50.1 112 112c0 44.7-26.2 83.2-64 101.2V416c0 17.7-14.3 32-32 32h-64c-17.7 0-32-14.3-32-32m8-328a56 56 0 1 1 112 0a56 56 0 1 1-112 0m120 157.7v84.7c10-11.3 16-26.1 16-42.3s-6-31.1-16-42.3zM320 32a64 64 0 1 1 0 128a64 64 0 1 1 0-128m-80 272c0 16.2 6 31 16 42.3v-84.7c-10 11.3-16 26.1-16 42.3zm144-42.3v84.7c10-11.3 16-26.1 16-42.3s-6-31.1-16-42.3zm64 42.3c0 44.7-26.2 83.2-64 101.2V448c0 17.7-14.3 32-32 32h-64c-17.7 0-32-14.3-32-32v-42.8c-37.8-18-64-56.5-64-101.2c0-61.9 50.1-112 112-112h32c61.9 0 112 50.1 112 112"/></svg>
        </button>
        <button class="back-to-select" onclick={() => worldMapVisible.update(v => !v)} title="World Map (M)">
          <svg xmlns="http://www.w3.org/2000/svg" width="576" height="512" viewBox="0 0 576 512"><path fill="currentColor" d="M384 476.1L192 421.2V35.9L384 90.8zM416 88.4V456l138.5-69.3c11.9-5.9 21.5-17.4 21.5-30.7V32c0-22-21.5-37.5-42.7-30.7L416 88.4zM160 421.2l-25.5-8.5C94 400.3 64 363.6 64 321.4V280h32c17.7 0 32-14.3 32-32s-14.3-32-32-32H64V192c0-17.7-14.3-32-32-32S0 174.3 0 192v129.4C0 383.5 38.3 439 91.3 457.2l68.7 22.9V88.4L21.2 33.7C9.3 39.6 0 51.1 0 64.4v1.6h32c17.7 0 32 14.3 32 32s-14.3 32-32 32H0v24h64c17.7 0 32 14.3 32 32s-14.3 32-32 32H0v105.4c0 62.1 38.3 117.6 91.3 135.8l68.7 22.9z"/></svg>
        </button>
        <button class="back-to-select" onclick={() => (showSettings = true)} title="Settings">
          <svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"><path fill="currentColor" d="M495.9 166.6c3.2 8.7 .5 18.4-6.4 24.6l-43.3 39.4c1.1 8.3 1.7 16.8 1.7 25.4s-.6 17.1-1.7 25.4l43.3 39.4c6.9 6.2 9.6 15.9 6.4 24.6c-4.4 11.9-9.7 23.3-15.8 34.3l-4.7 8.1c-6.6 11-14 21.4-22.1 31.2c-5.9 7.2-15.7 9.6-24.5 6.8l-55.7-17.7c-13.4 10.3-28.2 18.9-44 25.4l-12.5 57.1c-2 9.1-9 16.3-18.2 17.8c-13.8 2.3-28 3.5-42.5 3.5s-28.7-1.2-42.5-3.5c-9.2-1.5-16.2-8.7-18.2-17.8l-12.5-57.1c-15.8-6.5-30.6-15.1-44-25.4l-55.7 17.7c-8.8 2.8-18.6 .3-24.5-6.8c-8.1-9.8-15.5-20.2-22.1-31.2l-4.7-8.1c-6.1-11-11.4-22.4-15.8-34.3c-3.2-8.7-.5-18.4 6.4-24.6l43.3-39.4c-1.1-8.4-1.7-16.9-1.7-25.5s.6-17.1 1.7-25.4l-43.3-39.4c-6.9-6.2-9.6-15.9-6.4-24.6c4.4-11.9 9.7-23.3 15.8-34.3l4.7-8.1c6.6-11 14-21.4 22.1-31.2c5.9-7.2 15.7-9.6 24.5-6.8l55.7 17.7c13.4-10.3 28.2-18.9 44-25.4l12.5-57.1c2-9.1 9-16.3 18.2-17.8C227.3 1.2 241.5 0 256 0s28.7 1.2 42.5 3.5c9.2 1.5 16.2 8.7 18.2 17.8l12.5 57.1c15.8 6.5 30.6 15.1 44 25.4l55.7-17.7c8.8-2.8 18.6-.3 24.5 6.8c8.1 9.8 15.5 20.2 22.1 31.2l4.7 8.1c6.1 11 11.4 22.4 15.8 34.3zM256 336a80 80 0 1 0 0-160a80 80 0 1 0 0 160z"/></svg>
        </button>
      </div>
    </div>

    {#if isSceneCompiling || isCurrentPlayerLoading || $teleportLoading}
      <LoadingDialog message={isSceneCompiling ? 'Preparing world...' : 'Loading...'} />
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
      onStartGame={handleStartGame}
      onDeleteCharacter={handleDeleteCharacter}
      onLogout={handleLogoutToLogin}
    />
  {:else if screen === 'character-create'}
    <CharacterCreateScreen
      {accountName}
      characters={accountCharacters}
      selectedClass={createSelectedClass}
      onClassChange={(cls) => { createSelectedClass = cls }}
      onRollCharacterStats={handleRollCharacterStats}
      onCreateCharacter={handleCreateCharacter}
      onCharacterCreated={handleCharacterCreated}
      onCancel={handleCancelCreateCharacter}
    />
  {:else}
    <LoginScreen onLogin={handleLogin} {kickedMessage} />
  {/if}

  {#if screen !== 'game'}
    <button class="settings-btn-corner" class:raised={screen !== 'login'} onclick={() => (showSettings = true)} title="Settings">
      <svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"><path fill="currentColor" d="M495.9 166.6c3.2 8.7 .5 18.4-6.4 24.6l-43.3 39.4c1.1 8.3 1.7 16.8 1.7 25.4s-.6 17.1-1.7 25.4l43.3 39.4c6.9 6.2 9.6 15.9 6.4 24.6c-4.4 11.9-9.7 23.3-15.8 34.3l-4.7 8.1c-6.6 11-14 21.4-22.1 31.2c-5.9 7.2-15.7 9.6-24.5 6.8l-55.7-17.7c-13.4 10.3-28.2 18.9-44 25.4l-12.5 57.1c-2 9.1-9 16.3-18.2 17.8c-13.8 2.3-28 3.5-42.5 3.5s-28.7-1.2-42.5-3.5c-9.2-1.5-16.2-8.7-18.2-17.8l-12.5-57.1c-15.8-6.5-30.6-15.1-44-25.4l-55.7 17.7c-8.8 2.8-18.6 .3-24.5-6.8c-8.1-9.8-15.5-20.2-22.1-31.2l-4.7-8.1c-6.1-11-11.4-22.4-15.8-34.3c-3.2-8.7-.5-18.4 6.4-24.6l43.3-39.4c-1.1-8.4-1.7-16.9-1.7-25.5s.6-17.1 1.7-25.4l-43.3-39.4c-6.9-6.2-9.6-15.9-6.4-24.6c4.4-11.9 9.7-23.3 15.8-34.3l4.7-8.1c6.6-11 14-21.4 22.1-31.2c5.9-7.2 15.7-9.6 24.5-6.8l55.7 17.7c13.4-10.3 28.2-18.9 44-25.4l12.5-57.1c2-9.1 9-16.3 18.2-17.8C227.3 1.2 241.5 0 256 0s28.7 1.2 42.5 3.5c9.2 1.5 16.2 8.7 18.2 17.8l12.5 57.1c15.8 6.5 30.6 15.1 44 25.4l55.7-17.7c8.8-2.8 18.6-.3 24.5 6.8c8.1 9.8 15.5 20.2 22.1 31.2l4.7 8.1c6.1 11 11.4 22.4 15.8 34.3zM256 336a80 80 0 1 0 0-160a80 80 0 1 0 0 160z"/></svg>
    </button>
  {/if}

  {#if showSettings}
    <SettingsPanel onClose={() => (showSettings = false)} />
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

  .canvas-layer {
    position: absolute;
    inset: 0;
    z-index: 0;
    transition: filter 180ms ease;
  }

  .canvas-layer.dead {
    filter: grayscale(100%);
  }

  .game-hud {
    position: absolute;
    inset: 0;
    z-index: 1;
    pointer-events: none;
  }

  /* Allow pointer events on interactive HUD children */
  .game-hud :global(*) {
    pointer-events: auto;
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
    padding: 8px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .back-to-select svg {
    width: 20px;
    height: 20px;
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

  .settings-btn-corner {
    position: fixed;
    bottom: 16px;
    right: 16px;
    z-index: 9999;
    background: rgba(60, 60, 60, 0.85);
    color: #ccc;
    border: none;
    border-radius: 8px;
    padding: 8px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 150ms ease, color 150ms ease;
  }

  .settings-btn-corner:hover {
    background: rgba(80, 80, 80, 0.95);
    color: #fff;
  }

  .settings-btn-corner.raised {
    bottom: 80px;
  }

  .settings-btn-corner svg {
    width: 20px;
    height: 20px;
  }

</style>
