export function getDefaultServerUrl(): string {
  if (typeof window === 'undefined') return 'ws://localhost:5002'
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const hostname = window.location.hostname
  const port = window.location.port
  if (port) {
    return `${protocol}//${hostname}:${parseInt(port) + 1}`
  } else {
    return `${protocol}//${hostname}:5002`
  }
}

export function getTerrainApiUrl(): string {
  if (typeof window === 'undefined') return 'http://localhost:5003'
  // In dev, Vite proxies /api/terrain → http://localhost:5003
  // Use same origin so the request goes through the proxy
  return window.location.origin
}
