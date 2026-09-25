import { writable } from 'svelte/store'
import type { TravelDestination } from '../utils/autoTravel'

export const travelDestination = writable<TravelDestination | null>(null)
