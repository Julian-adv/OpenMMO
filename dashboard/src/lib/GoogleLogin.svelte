<script lang="ts">
  import { onMount } from 'svelte'
  import { useDashboardAuth } from './auth.svelte'

  interface GoogleIdentity {
    initialize(config: { client_id: string; callback: (response: { credential: string }) => void; auto_select: boolean }): void
    renderButton(container: HTMLElement, options: { theme: string; size: string; width: number }): void
    disableAutoSelect(): void
  }

  const auth = useDashboardAuth()
  let buttonContainer: HTMLDivElement
  let scriptError = $state('')

  onMount(() => {
    const clientId = import.meta.env.VITE_GOOGLE_CLIENT_ID as string | undefined
    if (!clientId) {
      scriptError = '로그인 설정이 필요합니다.'
      return
    }
    const googleId = () => (window as Window & { google?: { accounts: { id: GoogleIdentity } } }).google?.accounts.id
    let active = true
    let timeout: ReturnType<typeof setTimeout>
    const fail = () => {
      if (active) scriptError = 'Google 로그인을 불러오지 못했습니다. 페이지를 새로고침해 주세요.'
    }
    const render = () => {
      clearTimeout(timeout)
      if (!active) return
      const identity = googleId()
      if (!identity) return fail()
      scriptError = ''
      identity.initialize({
        client_id: clientId,
        callback: ({ credential }) => { if (active && !auth.pending) void auth.signIn(credential) },
        auto_select: false,
      })
      identity.disableAutoSelect()
      identity.renderButton(buttonContainer, { theme: 'outline', size: 'large', width: Math.min(280, buttonContainer.clientWidth) })
    }
    const src = 'https://accounts.google.com/gsi/client'
    let script = document.querySelector<HTMLScriptElement>(`script[src="${src}"]`)
    if (googleId()) {
      render()
    } else {
      const created = !script
      script ??= document.createElement('script')
      script.src = src
      script.async = true
      script.addEventListener('load', render)
      script.addEventListener('error', fail)
      timeout = setTimeout(fail, 10000)
      if (created) document.head.appendChild(script)
    }
    return () => {
      active = false
      clearTimeout(timeout)
      script?.removeEventListener('load', render)
      script?.removeEventListener('error', fail)
    }
  })
</script>

<main class="login-page">
  <section class="login-card" aria-labelledby="login-title" aria-busy={auth.pending}>
    <div class="eyebrow">OPENMMO PULSE</div>
    <h1 id="login-title">운영자 로그인<span>.</span></h1>
    <p>허용된 운영자 계정으로 로그인해 주세요.</p>
    <div class="google-button" class:pending={auth.pending} bind:this={buttonContainer}></div>
    {#if auth.pending}<p role="status">접근 권한을 확인하고 있습니다.</p>{/if}
    {#if auth.error || scriptError}<p class="login-error" role="alert">{auth.error || scriptError}</p>{/if}
  </section>
</main>

<style>
  .login-page { display: grid; place-items: center; min-height: 100dvh; padding: 24px; margin: 0 auto; }
  .login-card { width: min(100%, 440px); padding: 40px 28px; background: white; border: 1px solid #e2e9e5; border-radius: 16px; text-align: center; }
  .eyebrow { justify-content: center; }
  h1 { font-size: 28px; }
  p { font-size: 13px; line-height: 1.8; color: #74857d; }
  .google-button { display: flex; justify-content: center; min-height: 44px; margin-top: 28px; }
  .pending { pointer-events: none; opacity: .6; }
  .login-error { color: #a34532; margin-top: 20px; }
</style>
