import { writable } from 'svelte/store'

/** Object the server laid the local player on when it respawned them (a
 *  sick-room bed or an estate aid cot). The server already holds the claim,
 *  so `PlayerControl` enters the pose without sending `InteractObject`, then
 *  clears this. */
export const respawnPoseRequest = writable<{
  objectType: string
  objectId: number | null
} | null>(null)
