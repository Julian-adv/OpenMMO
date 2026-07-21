import { networkManager } from '../../network/socket'
import type { PositionCorrection } from '../../network/networkTypes'

export interface PlayerNetworkEventActions {
  /** True if the local player exists, is currently dead, and the id matches. */
  isCurrentPlayerEligibleForRespawn: () => boolean
  isCurrentPlayer: (playerId: number) => boolean
  isInteracting: () => boolean
  onRespawned: () => void
  onInteractionRejected: () => void
  onPositionCorrected: (correction: PositionCorrection) => void
}

/** Wires up respawn, interaction-rejected and position-correction listeners.
 *  Returns a cleanup. */
export function subscribePlayerNetworkEvents(
  actions: PlayerNetworkEventActions
): () => void {
  let respawnRequested = false

  const unsubscribeRespawnRequested = networkManager.respawnRequested.on(() => {
    if (!actions.isCurrentPlayerEligibleForRespawn() || respawnRequested) return
    respawnRequested = true
  })

  const unsubscribePlayerRespawned = networkManager.playerRespawned.on(
    (playerId) => {
      if (!actions.isCurrentPlayer(playerId)) return
      respawnRequested = false
      actions.onRespawned()
    }
  )

  const unsubscribeInteractionRejected = networkManager.interactionRejected.on(
    () => {
      if (actions.isInteracting()) actions.onInteractionRejected()
    }
  )

  const unsubscribePositionCorrected = networkManager.positionCorrected.on(
    actions.onPositionCorrected
  )

  return () => {
    unsubscribePositionCorrected()
    unsubscribeRespawnRequested()
    unsubscribePlayerRespawned()
    unsubscribeInteractionRejected()
  }
}
