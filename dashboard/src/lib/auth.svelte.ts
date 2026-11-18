import { getContext, setContext } from 'svelte'

const authContext = Symbol('dashboard-auth')
const sessionUrl = '/api/metrics/session'
const expiredMessage = '로그인이 만료되었습니다. 다시 로그인해 주세요.'
const deniedMessage = '대시보드 접근 권한이 없는 계정입니다. 운영자 계정으로 로그인해 주세요.'

export function provideDashboardAuth() {
  let signedIn = $state(false)
  let checking = $state(true)
  let pending = $state(false)
  let error = $state('')
  let attempt = 0

  function drop(message = '') {
    attempt++
    signedIn = false
    checking = false
    pending = false
    error = message
  }

  async function resume() {
    const currentAttempt = attempt
    try {
      const response = await fetch(sessionUrl, { cache: 'no-store', signal: AbortSignal.timeout(10000) })
      if (currentAttempt === attempt && response.status === 204) signedIn = true
    } catch {
      // The login screen is the fallback.
    } finally {
      if (currentAttempt === attempt) checking = false
    }
  }

  async function signIn(credential: string) {
    const currentAttempt = ++attempt
    checking = false
    pending = true
    error = ''
    try {
      const response = await fetch(sessionUrl, {
        method: 'POST',
        headers: { Authorization: `Bearer ${credential}` },
        cache: 'no-store',
        signal: AbortSignal.timeout(10000),
      })
      if (currentAttempt !== attempt) return
      if (response.status === 403) throw new Error(deniedMessage)
      if (response.status === 401) throw new Error(expiredMessage)
      if (response.status !== 204) throw new Error('로그인을 확인하지 못했습니다. 잠시 후 다시 시도해 주세요.')
      signedIn = true
    } catch (cause) {
      if (currentAttempt === attempt) {
        error = cause instanceof Error && cause.name === 'Error'
          ? cause.message : '로그인에 연결하지 못했습니다. 잠시 후 다시 시도해 주세요.'
      }
    } finally {
      if (currentAttempt === attempt) pending = false
    }
  }

  const auth = {
    get signedIn() { return signedIn },
    get checking() { return checking },
    get pending() { return pending },
    get error() { return error },
    signIn,
    signOut() {
      drop()
      void fetch(sessionUrl, { method: 'DELETE', cache: 'no-store', keepalive: true }).catch(() => {})
    },
    async fetch(url: string, init: RequestInit = {}) {
      if (!signedIn) throw new Error(expiredMessage)
      const currentAttempt = attempt
      const response = await fetch(url, { ...init, cache: 'no-store' })
      if (currentAttempt === attempt && (response.status === 401 || response.status === 403)) {
        drop(response.status === 403 ? deniedMessage : expiredMessage)
      }
      return response
    },
    dispose() {
      attempt++
    },
  }
  void resume()
  setContext(authContext, auth)
  return auth
}

export function useDashboardAuth() {
  return getContext<ReturnType<typeof provideDashboardAuth>>(authContext)
}
