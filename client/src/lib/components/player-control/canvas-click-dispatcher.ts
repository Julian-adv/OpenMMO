import type { Position } from '../../utils/movementUtils'
import type { ClickIntent } from '../../managers/inputHandler'
import type { WallDirection } from '../../utils/house-geometry'
import {
  PLAYER_ATTACK_RANGE_METERS,
  PLAYER_PICKUP_RANGE_METERS,
} from '../../data/combatTiming'

type InteractIntent = Extract<ClickIntent, { type: 'interact_object' }>
type PickupIntent = Extract<ClickIntent, { type: 'pickup_ground_item' }>
type NpcIntent = Extract<ClickIntent, { type: 'interact_npc' }>

export interface CanvasClickActions {
  /** Player is at melee range — start the attack swing immediately. */
  attackInRange(monsterId: string): void
  /** Out of range — chase the monster, attacking on arrival. */
  chaseAndAttack(monsterId: string, hitPoint: Position): void
  toggleDoor(
    houseId: string,
    roomIndex: number,
    wallDir: WallDirection,
    segmentIndex: number
  ): void
  enterInteraction(intent: InteractIntent): void
  enterPickup(instanceId: number): void
  approachAndPickup(intent: PickupIntent): void
  interactNpc(intent: NpcIntent): void
  moveToGround(position: Position): void
}

export function dispatchCanvasClickIntent(
  intent: ClickIntent,
  isMapEditorMode: boolean,
  actions: CanvasClickActions
): void {
  if (isMapEditorMode && intent.type !== 'move_to_ground') return

  switch (intent.type) {
    case 'attack_monster':
      if (intent.distance < PLAYER_ATTACK_RANGE_METERS) {
        actions.attackInRange(intent.monsterId)
      } else {
        actions.chaseAndAttack(intent.monsterId, intent.hitPoint)
      }
      return
    case 'toggle_door':
      actions.toggleDoor(
        intent.houseId,
        intent.roomIndex,
        intent.wallDir,
        intent.segmentIndex
      )
      return
    case 'interact_object':
      actions.enterInteraction(intent)
      return
    case 'pickup_ground_item':
      if (intent.distance <= PLAYER_PICKUP_RANGE_METERS) {
        actions.enterPickup(intent.instanceId)
      } else {
        actions.approachAndPickup(intent)
      }
      return
    case 'interact_npc':
      actions.interactNpc(intent)
      return
    case 'move_to_ground':
      actions.moveToGround(intent.position)
      return
    case 'none':
      return
    default: {
      const _exhaustive: never = intent
      return _exhaustive
    }
  }
}
