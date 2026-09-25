import { getContext, setContext } from 'svelte'

const authContext = Symbol('dashboard-auth')
const expiredMessage = '로그인이 만료되었습니다. 다시 로그인해 주세요.'
const deniedMessage = '대시보드 접근 권한이 없는 계정입니다. 운영자 계정으로 로그인해 주세요.'

export function provideDashboardAuth() {
  let token = $state('')
  let pending = $state(false)
  let error = $state('')
  let expirationTimer: ReturnType<typeof setTimeout> | undefined
  let attempt = 0

  function signOut(message = '') {
    attempt++
    clearTimeout(expirationTimer)
    token = ''
    pending = false
    error = message
  }

  async function signIn(credential: string) {
    const currentAttempt = ++attempt
    pending = true
    error = ''
    try {
      const response = await fetch('/api/metrics/session', {
        headers: { Authorization: `Bearer ${credential}` },
        cache: 'no-store',
        signal: AbortSignal.timeout(10000),
      })
      if (currentAttempt !== attempt) return
      if (response.status === 403) throw new Error(deniedMessage)
      if (response.status === 401) throw new Error(expiredMessage)
      if (response.status !== 204) throw new Error('로그인을 확인하지 못했습니다. 잠시 후 다시 시도해 주세요.')
      const payload = JSON.parse(atob(credential.split('.')[1].replace(/-/g, '+').replace(/_/g, '/'))) as { exp?: number }
      const expiresIn = typeof payload.exp === 'number' ? payload.exp * 1000 - Date.now() : 0
      if (!Number.isFinite(expiresIn) || expiresIn <= 0) throw new Error(expiredMessage)
      clearTimeout(expirationTimer)
      expirationTimer = setTimeout(() => signOut(expiredMessage), Math.min(expiresIn, 2147483647))
      token = credential
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
    get signedIn() { return !!token },
    get pending() { return pending },
    get error() { return error },
    signIn,
    signOut,
    async fetch(url: string, init: RequestInit = {}) {
      const credential = token
      if (!credential) throw new Error(expiredMessage)
      const headers = new Headers(init.headers)
      headers.set('Authorization', `Bearer ${credential}`)
      const response = await fetch(url, { ...init, headers, cache: 'no-store' })
      if (token === credential && (response.status === 401 || response.status === 403)) {
        signOut(response.status === 403 ? deniedMessage : expiredMessage)
      }
      return response
    },
    dispose() {
      attempt++
      clearTimeout(expirationTimer)
      token = ''
    },
  }
  setContext(authContext, auth)
  return auth
}

export function useDashboardAuth() {
  return getContext<ReturnType<typeof provideDashboardAuth>>(authContext)
}
