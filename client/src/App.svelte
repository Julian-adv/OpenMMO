<script lang="ts">
  import { Canvas } from '@threlte/core'
  import GameScene from './lib/components/GameScene.svelte'
  import ChatPanel from './lib/components/ChatPanel.svelte'
  import FPSCounter from './lib/components/FPSCounter.svelte'
  import GameTimeWidget from './lib/components/GameTimeWidget.svelte'
  import LoginScreen from './lib/components/LoginScreen.svelte'
  import CharacterSelectScreen from './lib/components/CharacterSelectScreen.svelte'
  import CharacterCreateScreen from './lib/components/CharacterCreateScreen.svelte'
  import RespawnDialog from './lib/components/RespawnDialog.svelte'
  import CharacterAttributesHud from './lib/components/CharacterAttributesHud.svelte'
  import { gameStore } from './lib/stores/gameStore'
  import { networkManager, type AccountCharacter } from './lib/network/socket'

  type AppScreen = 'login' | 'character-select' | 'character-create' | 'game'
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
  let showRespawnDialog = $state(false)
  let wasPlayerDead = false
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

  async function handleCreateCharacter(characterName: string) {
    const result = await networkManager.requestCreateCharacter(characterName)
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
    showRespawnDialog = false
    networkManager.requestRespawn()
  }

  function closeRespawnDialog() {
    showRespawnDialog = false
  }

  networkManager.onKicked((reason) => {
    kickedMessage = reason
    accountName = ''
    accountCharacters = []
    selectedCharacterId = null
    screen = 'login'
  })

  gameStore.subscribe((state) => {
    currentPlayerHp = state.currentPlayer?.health ?? null
    currentPlayerMaxHp = state.currentPlayer?.maxHealth ?? null
    const deadNow =
      screen === 'game' &&
      !!state.currentPlayer &&
      state.currentPlayer.health <= 0
    if (deadNow && !wasPlayerDead) {
      showRespawnDialog = true
    }
    if (!deadNow) {
      showRespawnDialog = false
    }
    isPlayerDead = deadNow
    wasPlayerDead = deadNow
  })
</script>

<main>
  {#if screen === 'game'}
    <div class="game-shell" class:dead={isPlayerDead}>
      <Canvas renderMode="always" shadows>
        <GameScene {serverUrl} />
      </Canvas>
      <ChatPanel />
      <FPSCounter />
      <GameTimeWidget />
      {#if selectedCharacter}
        <CharacterAttributesHud
          level={selectedCharacter.level}
          currentHp={currentPlayerHp ?? selectedCharacter.max_hp}
          maxHp={currentPlayerMaxHp ?? selectedCharacter.max_hp}
          attributes={selectedCharacter.attributes}
        />
      {/if}
      <button class="back-to-select" onclick={handleBackToCharacterSelect}>
        Character Select
      </button>
    </div>

    {#if showRespawnDialog}
      <RespawnDialog onRespawn={requestRespawn} onLater={closeRespawnDialog} />
    {:else if isPlayerDead}
      <button class="respawn-reopen" onclick={() => (showRespawnDialog = true)}>
        Respawn
      </button>
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

  .respawn-reopen {
    border: none;
    border-radius: 8px;
    padding: 10px 14px;
    font-size: 14px;
    cursor: pointer;
  }

  .respawn-reopen {
    background: #e2b93b;
    color: #1a1a1a;
    font-weight: 700;
  }

  .respawn-reopen {
    position: absolute;
    right: 16px;
    bottom: 16px;
    z-index: 31;
  }

  .back-to-select {
    position: absolute;
    right: 16px;
    bottom: 16px;
    z-index: 30;
    border: none;
    border-radius: 8px;
    padding: 10px 14px;
    font-size: 13px;
    cursor: pointer;
    background: rgba(60, 60, 60, 0.85);
    color: #ccc;
    font-weight: 600;
    transition: background 150ms ease, color 150ms ease;
  }

  .back-to-select:hover {
    background: rgba(80, 80, 80, 0.95);
    color: #fff;
  }

</style>
