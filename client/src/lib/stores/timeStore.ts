import { writable } from 'svelte/store'

export const timeScale = writable(1.0)
export const sunTimeScale = writable(1.0)
/** Debug offset applied to the game hour (in hours). 0 = no override. */
export const sunDebugOffset = writable(0)

export interface ServerGameTime {
  year: number
  month: number
  day: number
  hour: number
  minute: number
  isNight: boolean
}

export const serverGameTime = writable<ServerGameTime | null>(null)

export function setServerGameTime(gameTime: ServerGameTime) {
  serverGameTime.set(gameTime)
}

export function clearServerGameTime() {
  serverGameTime.set(null)
}
