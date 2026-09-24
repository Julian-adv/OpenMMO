import type { PendingApproach } from './approach'

export type PlayerControlStateName =
  | 'idle'
  | 'moving'
  | 'keyboard_moving'
  | 'attacking'
  | 'object_interacting'
  | 'picking_up'
  | 'dead'

export interface MovingStateData {
  approach: PendingApproach | null
}

export interface PickingUpStateData {
  /** Ground-item instance being picked up by the current pickup animation. */
  pendingPickupInstanceId: number
}

export type ControlState =
  | { name: 'idle' }
  | ({ name: 'moving' } & MovingStateData)
  | { name: 'keyboard_moving' }
  | { name: 'attacking' }
  | { name: 'object_interacting' }
  | ({ name: 'picking_up' } & PickingUpStateData)
  | { name: 'dead' }

export type MovingControlState = Extract<ControlState, { name: 'moving' }>
export type PickingUpControlState = Extract<
  ControlState,
  { name: 'picking_up' }
>
