import type { MovementState, Position } from '../../../utils/movementUtils'
import type { PathWaypoint } from './movement-substrate'

export type PlayerControlStateName =
  | 'idle'
  | 'moving'
  | 'keyboard_moving'
  | 'attacking'
  | 'object_interacting'
  | 'picking_up'
  | 'dead'
  | 'jump_feedback'

// ───────────────────────────────────────────────────────────────────────────
// Owned control state (state object holds its own data)
//
// The machine OWNS the active state. Movement data lives inside the `moving`
// state and the in-flight pickup id inside `picking_up` — leaving the state
// drops the data, so there are no separate flags to reset. Kinematic outputs
// (rotation, speed) are not state-membership data and stay on the adapter.
// ───────────────────────────────────────────────────────────────────────────

export interface MovingStateData {
  /** Current waypoint target (the immediate point being walked toward). */
  target: Position
  /** Acceleration/deceleration integrator toward `target`. */
  movementState: MovementState
  /** Full A* path; `target` is `waypoints[waypointIndex]`. */
  waypoints: PathWaypoint[]
  waypointIndex: number
  /** Monster position this path was routed to, when it came from a chase.
   *  Lives here so leaving the state — or a click starting a new path —
   *  invalidates it, the way the rest of this data works. */
  chaseGoal: Position | null
  /** Item instance to pick up once this move arrives (far-pickup approach). */
  pendingPickupAfterMove: number | null
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
  | { name: 'jump_feedback' }

export type MovingControlState = Extract<ControlState, { name: 'moving' }>
export type PickingUpControlState = Extract<
  ControlState,
  { name: 'picking_up' }
>
