import { SvelteURLSearchParams } from 'svelte/reactivity'
import type { Hours } from './metrics'

export function createMetricsResource<T, H extends Hours>(getHours: () => H, endpoint: string, parse: (value: unknown, hours: H, query: Record<string, string>) => T, errorLabel = '접속 현황', getQuery: () => Record<string, string> = () => ({})) {
  let history = $state<T | null>(null)
  let refreshing = $state(false)
  let error = $state('')
  let refresh = () => {}

  $effect(() => {
    const hours = getHours()
    const query = getQuery()
    const params = new SvelteURLSearchParams({ ...query, hours: String(hours) }).toString()
    let stopped = false
    let controller: AbortController | null = null
    history = null
    error = ''

    async function update() {
      if (controller) return
      const request = new AbortController()
      controller = request
      refreshing = true
      const timeout = window.setTimeout(() => request.abort(), 10000)
      try {
        const response = await fetch(`/api/metrics/${endpoint}?${params}`, { signal: request.signal, cache: 'no-store' })
        if (!response.ok) throw new Error(`HTTP ${response.status}`)
        const data = parse(await response.json(), hours, query)
        if (!stopped) {
          history = data
          error = ''
        }
      } catch {
        if (!stopped) error = `${errorLabel}을 불러오지 못했어요. 잠시 후 다시 시도해 주세요.`
      } finally {
        window.clearTimeout(timeout)
        controller = null
        if (!stopped) refreshing = false
      }
    }

    refresh = () => { void update() }
    void update()
    const timer = window.setInterval(() => { void update() }, 30000)
    return () => {
      stopped = true
      window.clearInterval(timer)
      controller?.abort()
    }
  })

  return {
    get history() { return history },
    get refreshing() { return refreshing },
    get error() { return error },
    get loading() { return history === null && !error },
    refresh() { refresh() },
  }
}
