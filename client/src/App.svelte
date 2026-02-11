<script lang="ts">
  import { Canvas } from '@threlte/core'
  import GameScene from './lib/components/GameScene.svelte'
  import ChatPanel from './lib/components/ChatPanel.svelte'
  import FPSCounter from './lib/components/FPSCounter.svelte'
  import LoginScreen from './lib/components/LoginScreen.svelte'
  import RespawnDialog from './lib/components/RespawnDialog.svelte'
  import { gameStore } from './lib/stores/gameStore'
  import { networkManager } from './lib/network/socket'

  let isLoggedIn = $state(false)
  let serverUrl = $state('')
  let playerName = $state('')
  let password = $state('')
  let isPlayerDead = $state(false)
  let showRespawnDialog = $state(false)
  let wasPlayerDead = false

  function handleLogin(url: string, name: string, pass: string) {
    serverUrl = url
    playerName = name
    password = pass
    isLoggedIn = true
  }

  function requestRespawn() {
    showRespawnDialog = false
    networkManager.requestRespawn()
  }

  function closeRespawnDialog() {
    showRespawnDialog = false
  }

  gameStore.subscribe((state) => {
    const deadNow =
      isLoggedIn && !!state.currentPlayer && state.currentPlayer.health <= 0
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
  {#if isLoggedIn}
    <div class="game-shell" class:dead={isPlayerDead}>
      <Canvas renderMode="always">
        <GameScene {serverUrl} {playerName} {password} />
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
  {:else}
    <LoginScreen onLogin={handleLogin} />
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
