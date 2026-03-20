<script lang="ts">
  import { onMount } from 'svelte'
  import { getDefaultServerUrl } from '../utils/networkUtils'

  const STORAGE_KEY_PLAYER = 'onlinerpg_lastPlayerName'

  interface Props {
    onLogin: (
      serverUrl: string,
      accountName: string,
      password: string,
      createAccount: boolean
    ) => Promise<{ ok: boolean; message?: string }>
    kickedMessage?: string
  }

  let { onLogin, kickedMessage }: Props = $props()

  let accountName = $state('')
  let password = $state('')
  let isConnecting = $state(false)
  let pendingAction = $state<'login' | 'create'>('login')
  let errorMessage = $state('')

  onMount(() => {
    const savedPlayerName = localStorage.getItem(STORAGE_KEY_PLAYER)
    if (savedPlayerName) {
      accountName = savedPlayerName
    }
  })

  function validateForm(): string | null {
    if (!accountName.trim()) {
      return 'Please enter account name'
    }

    if (!password.trim()) {
      return 'Please enter password'
    }

    return null
  }

  async function submit(createAccount: boolean) {
    const validationError = validateForm()
    if (validationError) {
      errorMessage = validationError
      return
    }

    errorMessage = ''
    isConnecting = true
    pendingAction = createAccount ? 'create' : 'login'

    const result = await onLogin(
      getDefaultServerUrl(),
      accountName.trim(),
      password.trim(),
      createAccount
    )

    if (!result.ok) {
      errorMessage = result.message ?? 'Authentication failed'
      isConnecting = false
      return
    }

    localStorage.setItem(STORAGE_KEY_PLAYER, accountName.trim())
  }

  function handleSubmit(event: Event) {
    event.preventDefault()
    void submit(false)
  }
</script>

<div class="login-container">
  <div class="login-wrapper">
    <svg class="arch-title" viewBox="-100 -80 1000 500" xmlns="http://www.w3.org/2000/svg">
      <defs>
        <path id="archPath" d="M 40,320 Q 400,0 760,320" fill="none" />
        <pattern id="flowerPattern" patternUnits="userSpaceOnUse" width="256" height="256">
          <image href="/textures/flowerx4.png" width="256" height="256" />
        </pattern>
      </defs>
      <text
        stroke="white"
        stroke-width="3"
        paint-order="stroke"
      >
        <textPath
          href="#archPath"
          startOffset="50%"
          text-anchor="middle"
          dominant-baseline="auto"
          fill="url(#flowerPattern)"
          font-family="'Black Han Sans', sans-serif"
          font-size="260"
        >봇들마을</textPath>
      </text>
    </svg>
    <h1 class="title">BottleField</h1>

  <div class="login-panel">

    {#if kickedMessage}
      <div class="kicked-message">{kickedMessage}</div>
    {/if}

    <form onsubmit={handleSubmit}>
      <div class="form-group">
        <label for="playerName">Account Name</label>
        <input
          type="text"
          id="playerName"
          bind:value={accountName}
          placeholder="Enter your account"
          disabled={isConnecting}
        />
      </div>

      <div class="form-group">
        <label for="password">Password</label>
        <input
          type="password"
          id="password"
          bind:value={password}
          placeholder="Enter password"
          disabled={isConnecting}
        />
      </div>

      {#if errorMessage}
        <div class="error-message">{errorMessage}</div>
      {/if}

      <div class="button-row">
        <button type="submit" class="login-button" disabled={isConnecting}>
          {isConnecting && pendingAction === 'login' ? 'Connecting...' : 'Login'}
        </button>
        <button
          type="button"
          class="create-button"
          disabled={isConnecting}
          onclick={() => void submit(true)}
        >
          {isConnecting && pendingAction === 'create'
            ? 'Creating...'
            : 'Create Account'}
        </button>
      </div>
    </form>
  </div>
  </div>
</div>

<style>
  .login-container {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    background: linear-gradient(135deg, #1a1a2e 0%, #16213e 50%, #0f3460 100%);
  }

  .login-wrapper {
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .arch-title {
    width: 800px;
    height: 350px;
    margin-bottom: -20px;
    filter: drop-shadow(0 4px 16px rgba(0, 0, 0, 0.6));
  }

  .title {
    margin: 0 0 20px 0;
    color: #a0aec0;
    font-size: 18px;
    font-weight: 400;
    text-align: center;
    letter-spacing: 6px;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .login-panel {
    width: 400px;
    padding: 40px;
    background: rgba(0, 0, 0, 0.8);
    border: 1px solid #4a5568;
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .form-group label {
    color: #a0aec0;
    font-size: 14px;
    font-weight: 500;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .form-group input {
    padding: 12px 14px;
    border: 1px solid #4a5568;
    border-radius: 6px;
    background: #1a202c;
    color: #ffffff;
    font-size: 14px;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    transition:
      border-color 0.2s,
      box-shadow 0.2s;
  }

  .form-group input:focus {
    outline: none;
    border-color: #4299e1;
    box-shadow: 0 0 0 3px rgba(66, 153, 225, 0.2);
  }

  .form-group input:disabled {
    opacity: 0.5;
  }

  .form-group input::placeholder {
    color: #718096;
  }

  .kicked-message {
    margin-bottom: 20px;
    padding: 12px 14px;
    background: rgba(236, 201, 75, 0.15);
    border: 1px solid #ecc94b;
    border-radius: 6px;
    color: #ecc94b;
    font-size: 13px;
    text-align: center;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .error-message {
    padding: 10px 14px;
    background: rgba(245, 101, 101, 0.2);
    border: 1px solid #fc8181;
    border-radius: 6px;
    color: #fc8181;
    font-size: 13px;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .login-button {
    padding: 14px 20px;
    border: none;
    border-radius: 6px;
    background: linear-gradient(135deg, #4299e1 0%, #3182ce 100%);
    color: #ffffff;
    font-size: 16px;
    font-weight: 600;
    cursor: pointer;
    transition:
      transform 0.2s,
      box-shadow 0.2s;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .login-button:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(66, 153, 225, 0.4);
  }

  .login-button:active:not(:disabled) {
    transform: translateY(0);
  }

  .login-button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .button-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .create-button {
    padding: 14px 20px;
    border: 1px solid #4a5568;
    border-radius: 6px;
    background: #2d3748;
    color: #ffffff;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    transition:
      transform 0.2s,
      box-shadow 0.2s;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .create-button:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(255, 255, 255, 0.15);
  }

  .create-button:active:not(:disabled) {
    transform: translateY(0);
  }

  .create-button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
