import { writable } from 'svelte/store'
import type { EstateChest } from '../network/networkTypes'

export const estateChests = writable(new Map<number, EstateChest>())
