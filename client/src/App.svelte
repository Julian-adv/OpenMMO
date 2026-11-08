<script lang="ts">
  import { t } from './lib/i18n'
  import { onMount } from 'svelte'
  import type SceneCanvasComponent from './lib/components/SceneCanvas.svelte'
  import LoginScreen from './lib/components/LoginScreen.svelte'
  import CharacterSelectScreen from './lib/components/CharacterSelectScreen.svelte'
  import type { CharacterSlotLayout } from './lib/utils/characterSelectLayout'
  import CharacterCreateScreen from './lib/components/CharacterCreateScreen.svelte'
  import RenameCharacterDialog from './lib/components/RenameCharacterDialog.svelte'
  import { gameStore } from './lib/stores/gameStore'
  import { localStr } from './lib/stores/inventoryStore'
  import {
    networkManager,
    type AccountCharacter,
    type CharacterClass,
    type CharacterAppearance,
    type Gender,
  } from './lib/network/socket'
  import { bgmMuted, startBgm } from './lib/managers/bgmManager'
  import SettingsPanel from './lib/components/SettingsPanel.svelte'
  import { runGpuBenchmark } from './lib/utils/gpuBenchmark'
  import {
    needsAutoQuality,
    qualityForOutcome,
    applyAutoQuality,
  } from './lib/stores/graphicsSettings'
  import { instrumentPanelVisible } from './lib/stores/instrumentStore'

  let showSettings = $state(false)
  let SceneCanvas = $state<typeof SceneCanvasComponent>()
  let gameScreen = $state<typeof import('./lib/components/game-screen')>()

  type AppScreen = 'login' | 'character-select' | 'character-create' | 'game'
  type DeathUiState =
    | 'alive'
    | 'waiting_dying'
    | 'dialog_open'
    | 'dialog_closed'
  let screen = $state<AppScreen>('login')
  let serverUrl = $state('')
  let accountName = $state('')
  let accountCharacters = $state<AccountCharacter[]>([])
  let selectedCharacterId = $state<number | null>(null)
  let characterSlotLayout = $state<CharacterSlotLayout[]>([])
  let selectedCharacter = $derived<AccountCharacter | null>(
    accountCharacters.find(
      (character) => character.id === selectedCharacterId
    ) ?? null
  )
  $effect(() => {
    if (selectedCharacter) localStr.set(selectedCharacter.attributes.str)
  })
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
  // Reconnected dead characters skip the dying animation.
  let hasObservedCurrentPlayerAlive = false
  let isCurrentPlayerLoading = $state(false)
  let isSceneCompiling = $state(true)
  let kickedMessage = $state('')

  // Start buttons and Canvas double-clicks share the rename dialog.
  let renameCharacterId = $state<number | null>(null)

  // Character create screen state
  let createSelectedClass = $state<CharacterClass>('knight')
  let createSelectedGender = $state<Gender>('male')
  let createAppearance = $state<CharacterAppearance>({
    face: 'default',
    hair: 'crop',
    hair_color_original: true,
    eye_color_original: true,
  })
  let createFacePreview = $state(false)

  // Benchmark once; preserve an existing graphics preference.
  let gpuProbePending = $state(needsAutoQuality())

  // Antialiasing must be chosen before creating the shared renderer.
  let showCanvas = $derived(screen !== 'login' && !gpuProbePending)

  // Handle Ctrl+M before browser tab mute, including under the Korean IME.
  function handleKeydown(event: KeyboardEvent) {
    if ($instrumentPanelVisible) return
    if (event.ctrlKey && !event.shiftKey && event.code === 'KeyM') {
      event.preventDefault()
      bgmMuted.update((m) => !m)
    }
  }

  onMount(() => {
    if (!gpuProbePending) return
    void runGpuBenchmark()
      .then((result) => {
        if (!result) return
        const level = qualityForOutcome(result)
        console.info(
          result === 'no-webgpu'
            ? `[GpuBenchmark] no WebGPU adapter (WebGL fallback) -> ${level}`
            : `[GpuBenchmark] score=${result.score.toFixed(1)} ` +
                `probe=${result.elapsedMs.toFixed(0)}ms -> ${level}`
        )
        applyAutoQuality(level)
      })
      .finally(() => {
        gpuProbePending = false
      })
  })

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
      selectedCharacterId =
        accountCharacters.length > 0 ? accountCharacters[0].id : null
    }
  })

  async function handleLogin(
    url: string,
    googleIdToken: string
  ): Promise<{ ok: boolean; message?: string }> {
    kickedMessage = ''
    try {
      SceneCanvas ??= (await import('./lib/components/SceneCanvas.svelte'))
        .default
    } catch (error) {
      console.error('Unable to load character screen', error)
      return { ok: false, message: $t('loading.failed') }
    }
    const result = await networkManager.requestAuthentication(
      url,
      googleIdToken
    )

    if (result.ok) {
      const characters = result.characters ?? []
      serverUrl = url
      accountName = result.accountName ?? ''
      accountCharacters = characters
      selectedCharacterId = characters.length > 0 ? characters[0].id : null
      screen = 'character-select'
      return { ok: true }
    }

    return result
  }

  async function handleCreateCharacter(
    characterName: string,
    characterClass: CharacterClass,
    gender: Gender,
    appearance: CharacterAppearance
  ) {
    const result = await networkManager.requestCreateCharacter(
      characterName,
      characterClass,
      gender,
      appearance
    )
    if (result.ok && result.character) {
      accountCharacters = [...accountCharacters, result.character]
    }
    return result
  }

  $effect(() =>
    networkManager.characterDeleted.on((characterId) => {
      accountCharacters = accountCharacters.filter((c) => c.id !== characterId)
    })
  )

  async function handleDeleteCharacter(characterId: number, cancel = false) {
    const result = await networkManager.requestDeleteCharacter(
      characterId,
      cancel
    )
    if (result.ok) {
      accountCharacters = accountCharacters.map((character) =>
        character.id === characterId
          ? { ...character, deletion_due_at: result.deletionDueAt ?? null }
          : character
      )
    }
    return result
  }

  async function handleRollCharacterStats(cls: CharacterClass, gender: Gender) {
    return networkManager.requestRollCharacterStats(cls, gender)
  }

  async function handleRenameCharacter(newName: string) {
    const characterId = renameCharacterId
    if (characterId === null) return { ok: false, message: 'No character' }

    const result = await networkManager.requestRenameCharacter(
      characterId,
      newName
    )
    if (!result.ok || !result.name) return result

    const renamed = result.name
    accountCharacters = accountCharacters.map((character) =>
      character.id === characterId ? { ...character, name: renamed } : character
    )
    renameCharacterId = null
    await handleStartGame(characterId)
    return result
  }

  async function handleStartGame(
    characterId: number
  ): Promise<{ ok: boolean; message?: string; renameRequired?: boolean }> {
    const startingAccount = accountName
    try {
      gameScreen ??= await import('./lib/components/game-screen')
    } catch (error) {
      console.error('Unable to load game screen', error)
      return { ok: false, message: $t('loading.failed') }
    }
    if (screen !== 'character-select' || accountName !== startingAccount)
      return { ok: false, message: $t('characterSelect.enterFailed') }
    const result = await networkManager.requestEnterGame(characterId)
    if (result.ok) {
      // Reset death tracking for each game session.
      deathUiState = 'alive'
      isPlayerDead = false
      wasPlayerDead = false
      hasObservedCurrentPlayerAlive = false
      isSceneCompiling = true
      screen = 'game'
      return result
    }
    if (result.renameRequired) {
      renameCharacterId = characterId
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

  function handleCharacterSlotClick(index: number) {
    const character = accountCharacters[index]
    if (character) handleSelectCharacter(character.id)
    else handleOpenCreateCharacterScreen()
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
    networkManager.clearSession()
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
    if (screen !== 'game' || !isPlayerDead || deathUiState !== 'waiting_dying')
      return
    deathUiState = 'dialog_open'
  }

  networkManager.kicked.on((reason) => {
    networkManager.clearSession()
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
  })

  // Re-evaluate on screen changes; JoinSuccess may have already set health.
  $effect(() => {
    const hp = currentPlayerHp
    const inGame = screen === 'game'
    const deadNow = inGame && hp !== null && hp <= 0
    if (deadNow && !wasPlayerDead) {
      // Only a live-to-dead transition plays the dying animation.
      deathUiState = hasObservedCurrentPlayerAlive
        ? 'waiting_dying'
        : 'dialog_open'
    }
    if (!deadNow) {
      deathUiState = 'alive'
    }
    if (inGame && hp !== null && hp > 0) {
      hasObservedCurrentPlayerAlive = true
    }
    isPlayerDead = deadNow
    wasPlayerDead = deadNow
  })
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<main onclick={startBgm}>
  <!-- Keep one renderer across character screens and the game. -->
  {#if showCanvas && SceneCanvas}
    <div class="canvas-layer" class:dead={screen === 'game' && isPlayerDead}>
      <SceneCanvas
        {screen}
        selection={{
          characters: accountCharacters,
          selectedCharacterId,
          onSlotClick: handleCharacterSlotClick,
          onSlotLayout: (layout) => (characterSlotLayout = layout),
          onSlotDoubleClick: (i) => {
            const c = accountCharacters[i]
            if (c) {
              handleSelectCharacter(c.id)
              void handleStartGame(c.id)
            }
          },
        }}
        creation={{
          characterClass: createSelectedClass,
          gender: createSelectedGender,
          appearance: createAppearance,
          facePreview: createFacePreview,
        }}
        game={{
          serverUrl,
          onCurrentPlayerDyingFinished: handleCurrentPlayerDyingFinished,
        }}
        GameScene={gameScreen?.GameScene}
        bind:isCurrentPlayerLoading
        bind:isSceneCompiling
      />
    </div>
  {/if}

  <!-- UI overlays (outside Canvas) -->
  {#if screen === 'game' && gameScreen}
    <gameScreen.GameHud
      {selectedCharacter}
      {currentPlayerLevel}
      {currentPlayerTotalXp}
      {currentPlayerHp}
      {currentPlayerMaxHp}
      {canReopenRespawnDialog}
      {showRespawnDialog}
      {isSceneCompiling}
      {isCurrentPlayerLoading}
      onReopenRespawnDialog={reopenRespawnDialog}
      onBackToCharacterSelect={handleBackToCharacterSelect}
      onRespawn={requestRespawn}
      onCloseRespawnDialog={closeRespawnDialog}
      onOpenSettings={() => (showSettings = true)}
    />
  {:else if screen === 'character-select'}
    <CharacterSelectScreen
      {accountName}
      characters={accountCharacters}
      {selectedCharacterId}
      slotLayout={characterSlotLayout}
      onSlotClick={handleCharacterSlotClick}
      onStartGame={handleStartGame}
      onDeleteCharacter={handleDeleteCharacter}
      onLogout={handleLogoutToLogin}
    />
    {#if renameCharacterId !== null}
      <RenameCharacterDialog
        onRename={handleRenameCharacter}
        onCancel={() => (renameCharacterId = null)}
      />
    {/if}
  {:else if screen === 'character-create'}
    <CharacterCreateScreen
      {accountName}
      characters={accountCharacters}
      selectedClass={createSelectedClass}
      selectedGender={createSelectedGender}
      appearance={createAppearance}
      facePreview={createFacePreview}
      onFacePreviewChange={(zoomed) => (createFacePreview = zoomed)}
      onAppearanceChange={(value) => {
        createAppearance = value
      }}
      onClassChange={(cls) => {
        createSelectedClass = cls
      }}
      onGenderChange={(g) => {
        createSelectedGender = g
      }}
      onRollCharacterStats={handleRollCharacterStats}
      onCreateCharacter={handleCreateCharacter}
      onCharacterCreated={handleCharacterCreated}
      onCancel={handleCancelCreateCharacter}
    />
  {:else}
    <LoginScreen onLogin={handleLogin} {kickedMessage} />
  {/if}

  {#if screen !== 'game'}
    <button
      class="settings-btn-corner"
      class:raised={screen === 'character-create'}
      onclick={() => (showSettings = true)}
      title={$t('settings.title')}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="512"
        height="512"
        viewBox="0 0 512 512"
        ><path
          fill="currentColor"
          d="M495.9 166.6c3.2 8.7 .5 18.4-6.4 24.6l-43.3 39.4c1.1 8.3 1.7 16.8 1.7 25.4s-.6 17.1-1.7 25.4l43.3 39.4c6.9 6.2 9.6 15.9 6.4 24.6c-4.4 11.9-9.7 23.3-15.8 34.3l-4.7 8.1c-6.6 11-14 21.4-22.1 31.2c-5.9 7.2-15.7 9.6-24.5 6.8l-55.7-17.7c-13.4 10.3-28.2 18.9-44 25.4l-12.5 57.1c-2 9.1-9 16.3-18.2 17.8c-13.8 2.3-28 3.5-42.5 3.5s-28.7-1.2-42.5-3.5c-9.2-1.5-16.2-8.7-18.2-17.8l-12.5-57.1c-15.8-6.5-30.6-15.1-44-25.4l-55.7 17.7c-8.8 2.8-18.6 .3-24.5-6.8c-8.1-9.8-15.5-20.2-22.1-31.2l-4.7-8.1c-6.1-11-11.4-22.4-15.8-34.3c-3.2-8.7-.5-18.4 6.4-24.6l43.3-39.4c-1.1-8.4-1.7-16.9-1.7-25.5s.6-17.1 1.7-25.4l-43.3-39.4c-6.9-6.2-9.6-15.9-6.4-24.6c4.4-11.9 9.7-23.3 15.8-34.3l4.7-8.1c6.6-11 14-21.4 22.1-31.2c5.9-7.2 15.7-9.6 24.5-6.8l55.7 17.7c13.4-10.3 28.2-18.9 44-25.4l12.5-57.1c2-9.1 9-16.3 18.2-17.8C227.3 1.2 241.5 0 256 0s28.7 1.2 42.5 3.5c9.2 1.5 16.2 8.7 18.2 17.8l12.5 57.1c15.8 6.5 30.6 15.1 44 25.4l55.7-17.7c8.8-2.8 18.6-.3 24.5 6.8c8.1 9.8 15.5 20.2 22.1 31.2l4.7 8.1c6.1 11 11.4 22.4 15.8 34.3zM256 336a80 80 0 1 0 0-160a80 80 0 1 0 0 160z"
        /></svg
      >
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
    width: 100%;
    max-width: 100vw;
    height: 100vh;
    height: 100dvh;
    overflow: hidden;
    position: relative;
    user-select: none;
    -webkit-user-select: none;
  }

  /* Text entry stays selectable despite the app-wide user-select: none */
  main :global(input),
  main :global(textarea) {
    user-select: text;
    -webkit-user-select: text;
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

  .settings-btn-corner {
    position: fixed;
    right: max(16px, calc(env(safe-area-inset-right) + 10px));
    bottom: max(16px, calc(env(safe-area-inset-bottom) + 10px));
    box-sizing: border-box;
    width: 36px;
    height: 36px;
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
    transition:
      background 150ms ease,
      color 150ms ease;
  }

  .settings-btn-corner:hover {
    background: rgba(80, 80, 80, 0.95);
    color: #fff;
  }

  .settings-btn-corner.raised {
    bottom: max(80px, calc(env(safe-area-inset-bottom) + 80px));
  }

  .settings-btn-corner svg {
    width: 20px;
    height: 20px;
  }
</style>
