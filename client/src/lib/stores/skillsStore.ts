import { writable } from 'svelte/store'
import type { SkillId, Skills } from '../network/networkTypes'

export type { SkillId, Skills }

export const skillsStore = writable<Skills>({ learned: [] })

export function resetSkillsStore() {
  skillsStore.set({ learned: [] })
}
