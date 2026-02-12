<script lang="ts">
  import { Canvas } from '@threlte/core'
  import GameScene from './lib/components/GameScene.svelte'
  import ChatPanel from './lib/components/ChatPanel.svelte'
  import FPSCounter from './lib/components/FPSCounter.svelte'
  import LoginScreen from './lib/components/LoginScreen.svelte'
  import CharacterSelectScreen from './lib/components/CharacterSelectScreen.svelte'
  import RespawnDialog from './lib/components/RespawnDialog.svelte'
  import { gameStore } from './lib/stores/gameStore'
  import {
    networkManager,
    type AccountCharacter,
  } from './lib/network/socket'

  type AppScreen = 'login' | 'character-select' | 'game'
  let screen = $state<AppScreen>('login')
  let serverUrl = $state('')
  let accountName = $state('')
  let accountCharacters = $state<AccountCharacter[]>([])
  let isPlayerDead = $state(false)
  let showRespawnDialog = $state(false)
  let wasPlayerDead = false
  let kickedMessage = $state('')

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
      serverUrl = url
      accountName = result.accountName ?? account
      accountCharacters = result.characters ?? []
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

  async function handleStartGame(
    characterId: number
  ): Promise<{ ok: boolean; message?: string }> {
    const result = await networkManager.requestEnterGame(characterId)
    if (result.ok) {
      screen = 'game'
    }
    return result
  }

  function handleLogoutToLogin() {
    networkManager.disconnect()
    accountName = ''
    accountCharacters = []
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
    screen = 'login'
  })

  gameStore.subscribe((state) => {
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
      <Canvas renderMode="always">
        <GameScene {serverUrl} />
      </Canvas>
      <ChatPanel />
      <FPSCounter />
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
      onCreateCharacter={handleCreateCharacter}
      onStartGame={handleStartGame}
      onLogout={handleLogoutToLogin}
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
</style>
