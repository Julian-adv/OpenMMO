import { writable } from 'svelte/store'

/** Estate furniture the defeated can wake on. */
export const AID_COT = 'furniture_aid_cot'

export type RespawnPlace =
  | 'Town'
  | 'Estate'
  | 'FamilySeat'
  | { Nearby: { estate_id: number } }

export type ShelterAccess = 'Estate' | 'Family' | 'Public'

export const SHELTER_ACCESS: ShelterAccess[] = ['Estate', 'Family', 'Public']

export const shelterAccessKey = (access: ShelterAccess) =>
  `shelter.access${access}` as const

export type NearbyShelter = {
  estate_id: number
  name: string
  distance_m: number
}

export type RespawnOptions = {
  estate: boolean
  family_seat: boolean
  nearby: NearbyShelter | null
}

/** A public shelter on the world map. */
export type ShelterMarker = {
  estate_id: number
  name: string
  x: number
  z: number
}

/** Where the server says the defeated player may wake besides the inn. */
export const respawnOptions = writable<RespawnOptions | null>(null)

/** The signed-in account's character ids; estates belong to the account. */
export const accountCharacterIds = writable<number[]>([])
