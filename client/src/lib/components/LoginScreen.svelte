<script lang="ts">
  interface Props {
    onLogin: (serverUrl: string, playerName: string, password: string) => void
  }

  let { onLogin }: Props = $props()

  let serverUrl = $state('ws://127.0.0.1:8080')
  let playerName = $state('')
  let password = $state('')
  let isConnecting = $state(false)
  let errorMessage = $state('')

  function handleSubmit(event: Event) {
    event.preventDefault()

    if (!serverUrl.trim()) {
      errorMessage = 'Please enter server address'
      return
    }

    if (!playerName.trim()) {
      errorMessage = 'Please enter player name'
      return
    }

    if (!password.trim()) {
      errorMessage = 'Please enter password'
      return
    }

    errorMessage = ''
    isConnecting = true

    onLogin(serverUrl.trim(), playerName.trim(), password.trim())
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      handleSubmit(event)
    }
  }
</script>

<div class="login-container">
  <div class="login-panel">
    <h1 class="title">Online RPG</h1>

    <form onsubmit={handleSubmit}>
      <div class="form-group">
        <label for="serverUrl">Server Address</label>
        <input
          type="text"
          id="serverUrl"
          bind:value={serverUrl}
          placeholder="ws://192.168.0.17:8080"
          disabled={isConnecting}
        />
      </div>

      <div class="form-group">
        <label for="playerName">Player Name</label>
        <input
          type="text"
          id="playerName"
          bind:value={playerName}
          placeholder="Enter your name"
          disabled={isConnecting}
          onkeydown={handleKeyDown}
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
          onkeydown={handleKeyDown}
        />
      </div>

      {#if errorMessage}
        <div class="error-message">{errorMessage}</div>
      {/if}

      <button type="submit" class="login-button" disabled={isConnecting}>
        {isConnecting ? 'Connecting...' : 'Connect'}
      </button>
    </form>
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
    justify-content: center;
    align-items: center;
    background: linear-gradient(135deg, #1a1a2e 0%, #16213e 50%, #0f3460 100%);
  }

  .login-panel {
    width: 400px;
    padding: 40px;
    background: rgba(0, 0, 0, 0.8);
    border: 1px solid #4a5568;
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  }

  .title {
    margin: 0 0 30px 0;
    color: #ffffff;
    font-size: 28px;
    font-weight: 700;
    text-align: center;
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      Roboto,
      sans-serif;
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
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      Roboto,
      sans-serif;
  }

  .form-group input {
    padding: 12px 14px;
    border: 1px solid #4a5568;
    border-radius: 6px;
    background: #1a202c;
    color: #ffffff;
    font-size: 14px;
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      Roboto,
      sans-serif;
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

  .error-message {
    padding: 10px 14px;
    background: rgba(245, 101, 101, 0.2);
    border: 1px solid #fc8181;
    border-radius: 6px;
    color: #fc8181;
    font-size: 13px;
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      Roboto,
      sans-serif;
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
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      Roboto,
      sans-serif;
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
</style>
