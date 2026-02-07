export function getDefaultServerUrl(): string {
  if (typeof window === 'undefined') return 'ws://localhost:8080'
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  const hostname = window.location.hostname
  const port = window.location.port
  if (port) {
    return `${protocol}//${hostname}:${parseInt(port) - 1}`
  } else {
    return `${protocol}//${hostname}:8080`
  }
}
