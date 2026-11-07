<script lang="ts">
  import { t } from '../i18n'
  import { onMount } from 'svelte'
  import { getDefaultServerUrl } from '../utils/networkUtils'
  import AnnouncementsPanel from './AnnouncementsPanel.svelte'
  import LoginBackdrop from './LoginBackdrop.svelte'

  const GSI_SRC = 'https://accounts.google.com/gsi/client'

  interface Props {
    onLogin: (
      serverUrl: string,
      googleIdToken: string
    ) => Promise<{ ok: boolean; message?: string }>
    kickedMessage?: string
  }

  let { onLogin, kickedMessage }: Props = $props()

  let isConnecting = $state(false)
  let errorMessage = $state('')
  let buttonContainer = $state<HTMLDivElement | null>(null)

  function loadGsiScript(): Promise<void> {
    if (window.google?.accounts?.id) return Promise.resolve()
    return new Promise((resolve, reject) => {
      const existing = document.querySelector(`script[src="${GSI_SRC}"]`)
      if (existing) {
        existing.addEventListener('load', () => resolve())
        existing.addEventListener('error', () =>
          reject(new Error($t('login.googleLoad')))
        )
        return
      }
      const script = document.createElement('script')
      script.src = GSI_SRC
      script.async = true
      script.onload = () => resolve()
      script.onerror = () => reject(new Error($t('login.googleLoad')))
      document.head.appendChild(script)
    })
  }

  async function handleCredential(response: GoogleCredentialResponse) {
    errorMessage = ''
    isConnecting = true

    try {
      const result = await onLogin(getDefaultServerUrl(), response.credential)
      if (!result.ok) {
        errorMessage = result.message ?? $t('login.failed')
      }
    } catch (e) {
      // onLogin can reject (e.g. WASM init failure) — without this the button
      // stays disabled with no message.
      errorMessage = e instanceof Error ? e.message : $t('login.failed')
    } finally {
      isConnecting = false
    }
  }

  onMount(async () => {
    const devLogin = import.meta.env.DEV && import.meta.env.VITE_DEV_LOGIN
    const devAccount = new URLSearchParams(location.search).get('dev_login')
    if (devLogin && devAccount !== null) {
      // `?dev_login=<account>` signs in as another account; the secret is shared.
      const credential = devAccount
        ? (devLogin as string).replace(/^dev:[^:]*:/, `dev:${devAccount}:`)
        : (devLogin as string)
      await handleCredential({ credential })
      return
    }

    const clientId = import.meta.env.VITE_GOOGLE_CLIENT_ID as string | undefined
    // Comparing against the container build's placeholder is not possible here:
    // Vite inlines this, so the check would fold to a constant and let the
    // minifier drop the placeholder the entrypoint has to substitute.
    if (!clientId) {
      errorMessage = $t('login.unavailable')
      return
    }

    try {
      await loadGsiScript()
    } catch (e) {
      errorMessage = e instanceof Error ? e.message : String(e)
      return
    }

    const googleId = window.google?.accounts?.id
    if (!googleId || !buttonContainer) {
      errorMessage = $t('login.googleInit')
      return
    }

    googleId.initialize({
      client_id: clientId,
      callback: (response) => void handleCredential(response),
      auto_select: true,
      itp_support: true,
      use_fedcm_for_prompt: true,
    })
    // Auto sign-in for returning users; button below is the fallback.
    googleId.prompt()
    googleId.renderButton(buttonContainer, {
      theme: 'filled_blue',
      size: 'large',
      text: 'signin_with',
      shape: 'pill',
      width: 280,
    })
  })
</script>

<LoginBackdrop />
<div class="login-container">
  <div class="login-wrapper">
    <div class="hero">
      <svg
        class="arch-title"
        viewBox="-160 -80 1320 500"
        xmlns="http://www.w3.org/2000/svg"
      >
        <defs>
          <path id="archPath" d="M -20,360 Q 500,90 1020,360" fill="none" />
          <linearGradient id="titleGold" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color="#fff6d8" />
            <stop offset="0.45" stop-color="#f6c66a" />
            <stop offset="0.55" stop-color="#d98a2b" />
            <stop offset="1" stop-color="#8a4a1c" />
          </linearGradient>
          <linearGradient
            id="titleShine"
            gradientUnits="userSpaceOnUse"
            x1="-300"
            y1="0"
            x2="-60"
            y2="120"
          >
            <stop offset="0" stop-color="#fff" stop-opacity="0" />
            <stop offset="0.5" stop-color="#fff" stop-opacity="0.85" />
            <stop offset="1" stop-color="#fff" stop-opacity="0" />
            <animateTransform
              attributeName="gradientTransform"
              type="translate"
              values="0 0; 1600 0; 1600 0"
              keyTimes="0; 0.4; 1"
              dur="5s"
              begin="1.2s"
              repeatCount="indefinite"
            />
          </linearGradient>
          <text
            id="titleText"
            font-family="'Black Han Sans', sans-serif"
            font-size="200"
          >
            <textPath href="#archPath" startOffset="50%" text-anchor="middle"
              >Open<tspan dx="40">MMO</tspan></textPath
            >
          </text>
        </defs>
        <use
          href="#titleText"
          stroke="#2a1608"
          stroke-width="10"
          stroke-linejoin="round"
          paint-order="stroke"
          fill="url(#titleGold)"
        />
        <use class="title-shine" href="#titleText" fill="url(#titleShine)" />
      </svg>
      <h1 class="title">OpenMMO</h1>
      <p class="tagline">{$t('login.tagline')}</p>

      <div class="login-panel">
        {#if kickedMessage}
          <div class="kicked-message">{kickedMessage}</div>
        {/if}

        {#if errorMessage}
          <div class="error-message">{errorMessage}</div>
        {/if}

        <div class="google-signin" class:connecting={isConnecting}>
          <div bind:this={buttonContainer}></div>
          {#if isConnecting}
            <div class="connecting-label">{$t('login.connecting')}</div>
          {/if}
        </div>
      </div>
    </div>

    <div class="side">
      <AnnouncementsPanel />
      <nav class="site-links">
        <a
          class="site-link"
          href="https://julian-adv.github.io/blog/"
          target="_blank"
          rel="noopener noreferrer"
        >
          <svg
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M2 4h6a4 4 0 0 1 4 4v13a3 3 0 0 0-3-3H2z" />
            <path d="M22 4h-6a4 4 0 0 0-4 4v13a3 3 0 0 1 3-3h7z" />
          </svg>
          {$t('login.devlog')}
        </a>
        <a
          class="site-link"
          href="https://github.com/Julian-adv/OpenMMO"
          target="_blank"
          rel="noopener noreferrer"
        >
          <svg
            viewBox="0 0 16 16"
            width="20"
            height="20"
            fill="currentColor"
            aria-hidden="true"
          >
            <path
              d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"
            />
          </svg>
          {$t('login.sourceCode')}
        </a>
      </nav>
    </div>
  </div>
</div>

<style>
  .login-container {
    position: fixed;
    inset: 0;
    box-sizing: border-box;
    width: 100%;
    max-width: 100vw;
    height: 100vh;
    height: 100dvh;
    padding: max(14px, env(safe-area-inset-top))
      max(14px, env(safe-area-inset-right))
      max(58px, calc(env(safe-area-inset-bottom) + 58px))
      max(14px, env(safe-area-inset-left));
    overflow-x: hidden;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    -webkit-overflow-scrolling: touch;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .site-links {
    margin-top: 10px;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .site-link {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
    color: #a0aec0;
    font-size: 15px;
    text-decoration: none;
    transition: color 0.15s;
  }

  .site-link:hover {
    color: #fff;
  }

  .login-wrapper {
    position: relative;
    z-index: 1;
    width: min(800px, 100%);
    min-width: 0;
    margin: auto 0;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .hero,
  .side {
    width: 100%;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  @media (min-width: 1280px) {
    .login-wrapper {
      width: min(1320px, 100%);
      flex-direction: row;
      justify-content: center;
      gap: 40px;
    }

    .hero {
      flex: 0 1 800px;
    }

    .side {
      flex: 0 1 440px;
      animation: fade-up 0.7s ease-out 1.2s both;
    }
  }

  .arch-title {
    width: min(800px, 100%);
    height: auto;
    aspect-ratio: 1320 / 500;
    margin-bottom: -20px;
    filter: drop-shadow(0 4px 16px rgba(0, 0, 0, 0.6));
    animation:
      rise 1s ease-out both,
      float 6s ease-in-out 1s infinite,
      glow 4s ease-in-out 1s infinite;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(24px) scale(0.96);
    }
  }

  @keyframes float {
    50% {
      transform: translateY(-6px);
    }
  }

  @keyframes glow {
    50% {
      filter: drop-shadow(0 4px 16px rgba(0, 0, 0, 0.6))
        drop-shadow(0 0 18px rgba(255, 200, 120, 0.45));
    }
  }

  .tagline {
    margin: -8px 0 22px;
    max-width: 560px;
    color: #e2d9f3;
    font-size: 16px;
    line-height: 1.5;
    text-align: center;
    text-shadow: 0 2px 8px rgba(0, 0, 0, 0.8);
    animation: fade-up 0.8s ease-out 0.4s both;
  }

  @media (prefers-reduced-motion: reduce) {
    .title-shine {
      display: none;
    }

    .arch-title,
    .tagline,
    .login-panel {
      animation: none;
    }
  }

  .title {
    margin: 0 0 20px 0;
    color: #a0aec0;
    font-size: 18px;
    font-weight: 400;
    text-align: center;
    letter-spacing: 6px;
  }

  .login-panel {
    box-sizing: border-box;
    width: min(480px, 100%);
    min-width: 0;
    padding: 40px;
    background: rgba(0, 0, 0, 0.8);
    border: 1px solid #4a5568;
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    animation: fade-up 0.7s ease-out 0.55s both;
  }

  .google-signin {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    min-height: 44px;
  }

  .google-signin.connecting {
    pointer-events: none;
    opacity: 0.6;
  }

  .connecting-label {
    color: #a0aec0;
    font-size: 13px;
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
    user-select: text;
    -webkit-user-select: text;
  }

  .error-message {
    margin-bottom: 16px;
    padding: 10px 14px;
    background: rgba(245, 101, 101, 0.2);
    border: 1px solid #fc8181;
    border-radius: 6px;
    color: #fc8181;
    font-size: 13px;
    user-select: text;
    -webkit-user-select: text;
  }

  @media (max-width: 600px), (max-height: 700px) {
    .login-container {
      padding: max(10px, env(safe-area-inset-top))
        max(10px, env(safe-area-inset-right))
        max(52px, calc(env(safe-area-inset-bottom) + 52px))
        max(10px, env(safe-area-inset-left));
    }

    .arch-title {
      width: min(340px, 100%);
      margin-bottom: -12px;
    }

    .title {
      margin-bottom: 12px;
      font-size: 13px;
      letter-spacing: 4px;
    }

    .login-panel {
      width: min(320px, 100%);
      padding: 18px;
      border-radius: 8px;
    }

    .kicked-message {
      margin-bottom: 14px;
      padding: 10px 12px;
    }

    .tagline {
      margin: -4px 0 14px;
      font-size: 13px;
    }
  }

  @media (max-height: 560px) {
    .arch-title {
      width: min(320px, 100%);
      margin-bottom: -10px;
    }

    .login-panel {
      padding: 16px;
    }
  }
</style>
