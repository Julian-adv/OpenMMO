import { writable } from 'svelte/store'

export type ManaSnapshot = { mana: number; max_mana: number }
export const manaState = writable<ManaSnapshot | null>(null)
