import { writable } from 'svelte/store'

/// The page watches its account's character instead of playing it.
export const watchMode =
  typeof location !== 'undefined' &&
  new URLSearchParams(location.search).has('watch')
/// Watching, and the account has no character in the world right now.
export const watchWaiting = writable(false)
/// The watched character, drawn as any other player would see it. The page's
/// own player takes `WATCHER_SELF_ID`, so no self-only path claims it.
export const watchedPlayerId = writable<number | null>(null)
export const WATCHER_SELF_ID = -1
