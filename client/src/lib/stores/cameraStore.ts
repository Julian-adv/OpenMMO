import { writable } from 'svelte/store'

export const cameraDistance = writable(0)
export const cameraResetNonce = writable(0)

export const requestCameraReset = () => {
  cameraResetNonce.update((nonce) => nonce + 1)
}
