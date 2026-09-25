import { writable } from 'svelte/store'

export const SKILL_FAILURE_DURATION_MS = 2500

export const skillFailure = writable<{ id: number; text: string } | null>(null)
let nextId = 0
let timeout: ReturnType<typeof setTimeout> | undefined

export function clearSkillFailure() {
  clearTimeout(timeout)
  timeout = undefined
  skillFailure.set(null)
}

export function showSkillFailure(text: string) {
  clearTimeout(timeout)
  skillFailure.set({ id: ++nextId, text })
  timeout = setTimeout(clearSkillFailure, SKILL_FAILURE_DURATION_MS)
}
