import { networkManager } from '../../network/socket'

export interface PlayerNetworkEventActions {
  /** True if the local player exists, is currently dead, and the id matches. */
  isCurrentPlayerEligibleForRespawn: () => boolean
  isCurrentPlayer: (playerId: number) => boolean
  isInteracting: () => boolean
  onRespawned: () => void
  onInteractionRejected: () => void
}

/** Subscribe to player lifecycle events. */
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

  return () => {
    unsubscribeRespawnRequested()
    unsubscribePlayerRespawned()
    unsubscribeInteractionRejected()
  }
}
