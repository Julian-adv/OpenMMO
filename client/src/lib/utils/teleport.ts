import { gameStore } from '../stores/gameStore'
import { teleportLoading } from '../stores/debugStore'
import { networkManager } from '../network/socket'
import { wrapWorldX } from '../terrain/world-wrap'

/** Request an admin teleport and load the destination terrain. */
export function teleportLocalPlayer(x: number, y: number, z: number): number {
  const wrappedX = wrapWorldX(x)
  gameStore.update((state) => {
    state.currentPlayer?.position.set(wrappedX, y, z)
    return state
  })
  networkManager.sendDebugTeleport({ x: wrappedX, y, z })
  teleportLoading.set(true)
  return wrappedX
}
