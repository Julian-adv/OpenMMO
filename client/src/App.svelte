<script lang="ts">
  import { Canvas } from '@threlte/core'
  import GameScene from './lib/components/GameScene.svelte'
  import ChatPanel from './lib/components/ChatPanel.svelte'
  import FPSCounter from './lib/components/FPSCounter.svelte'
  import LoginScreen from './lib/components/LoginScreen.svelte'

  let isLoggedIn = $state(false)
  let serverUrl = $state('')
  let playerName = $state('')
  let password = $state('')

  function handleLogin(url: string, name: string, pass: string) {
    serverUrl = url
    playerName = name
    password = pass
    isLoggedIn = true
  }
</script>

<main>
  {#if isLoggedIn}
    <Canvas renderMode="always">
      <GameScene {serverUrl} {playerName} {password} />
    </Canvas>
    <ChatPanel />
    <FPSCounter />
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
</style>
