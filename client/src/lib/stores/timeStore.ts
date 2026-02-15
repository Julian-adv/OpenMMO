import { writable } from 'svelte/store'

export const timeScale = writable(1.0)
export const sunTimeScale = writable(1.0)
export const serverGameHour = writable<number | null>(null)

export function setServerGameHour(hour: number) {
  serverGameHour.set(hour)
}

export function clearServerGameHour() {
  serverGameHour.set(null)
}
