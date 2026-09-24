import type {
  MoveGoal,
  MoveDirection,
  MovePath,
  MoveProgress,
  MoveWaypoint,
  Position,
} from '../../network/networkTypes'
import { shortestWrappedDeltaX, wrapWorldX } from '../../terrain/world-wrap'

const SEND_INTERVAL_MS = 100
const MAX_EXTRAPOLATION_MS = 500

type Pose = {
  position: Position
  rotation: number
  speed: number
}

export class ServerMovement {
  private directionInput: MoveDirection | null = null
  private requestId: number | null = null
  private stopId: number | null = null
  private waypoints: MoveWaypoint[] = []
  private anchor: MoveProgress | null = null
  private anchorAt = 0
  private blockedPose: Pose | null = null
  private nextSendAt = 0
  private pending: MoveGoal | null = null
  private timer: ReturnType<typeof setTimeout> | null = null

  constructor(
    private readonly nextId: () => number,
    private readonly sendGoal: (goal: MoveGoal) => void,
    private readonly sendStop: (requestId: number) => void,
    private readonly sendDirection: (input: MoveDirection) => void = () => {}
  ) {}

  get active() {
    return this.requestId !== null
  }

  request(x: number, z: number, sprinting: boolean, stopAtEntrance = false) {
    if (!Number.isFinite(x) || !Number.isFinite(z)) return
    this.directionInput = null
    this.stopId = null
    this.requestId = this.nextId()
    this.anchor = null
    this.blockedPose = null
    this.waypoints = []
    this.pending = {
      request_id: this.requestId,
      x: wrapWorldX(x),
      z,
      sprinting,
      stop_at_entrance: stopAtEntrance,
    }
    const delay = this.nextSendAt - performance.now()
    if (delay <= 0) this.flush()
    else if (this.timer === null)
      this.timer = setTimeout(() => this.flush(), delay)
  }

  direction(input: Omit<MoveDirection, 'request_id'>) {
    const previous = this.directionInput
    if (
      !previous ||
      previous.rotation !== input.rotation ||
      previous.forward !== input.forward ||
      previous.turn !== input.turn ||
      previous.sprinting !== input.sprinting
    ) {
      this.clear(false)
      this.requestId = this.nextId()
      this.directionInput = { ...input, request_id: this.requestId }
    }
    this.sendDirection(this.directionInput!)
  }

  private flush() {
    if (this.timer !== null) clearTimeout(this.timer)
    this.timer = null
    if (!this.pending) return
    this.sendGoal(this.pending)
    this.pending = null
    this.nextSendAt = performance.now() + SEND_INTERVAL_MS
  }

  acceptPath(path: MovePath): boolean {
    if (
      path.request_id !== this.requestId ||
      path.server_time_ms < (this.anchor?.server_time_ms ?? 0)
    )
      return false
    this.waypoints = path.waypoints
    this.blockedPose = null
    this.anchor = { ...path, next_waypoint: 0, status: 'moving' }
    this.anchorAt = performance.now()
    return true
  }

  acceptStopped(progress: MoveProgress): boolean {
    if (progress.request_id !== this.stopId || progress.status !== 'stopped')
      return false
    this.stopId = null
    return true
  }

  acceptProgress(progress: MoveProgress): boolean {
    if (
      progress.request_id !== this.requestId ||
      progress.server_time_ms < (this.anchor?.server_time_ms ?? 0)
    )
      return false
    this.anchor = progress
    this.blockedPose = null
    this.anchorAt = performance.now()
    return true
  }

  sample(blocked: (from: Position, to: Position) => boolean): Pose | null {
    if (this.blockedPose) return this.blockedPose
    const anchor = this.anchor
    if (!anchor) return null
    let position = { ...anchor.position }
    let rotation = anchor.rotation
    let speed = this.waypoints.length > anchor.next_waypoint ? anchor.speed : 0
    const elapsedMs = performance.now() - this.anchorAt
    let elapsed = Math.min(elapsedMs, MAX_EXTRAPOLATION_MS) / 1000
    let budget = speed * elapsed
    if (anchor.status === 'moving') {
      for (
        let index = anchor.next_waypoint;
        index < this.waypoints.length;
        index++
      ) {
        const waypoint = this.waypoints[index]
        const target = waypoint.position
        const dx = shortestWrappedDeltaX(position.x, target.x)
        const dz = target.z - position.z
        const length = Math.hypot(dx, dz)
        if (waypoint.travel_seconds != null)
          speed = length / Math.max(waypoint.travel_seconds, 1e-5)
        const fraction =
          waypoint.travel_seconds != null
            ? Math.min(elapsed / waypoint.travel_seconds, 1)
            : length < 1e-5
              ? 1
              : Math.min(budget / length, 1)
        const next = {
          x: wrapWorldX(position.x + dx * fraction),
          y: position.y + (target.y - position.y) * fraction,
          z: position.z + dz * fraction,
        }
        if (blocked(position, next)) {
          speed = 0
          this.blockedPose = { position, rotation, speed }
          break
        }
        position = next
        if (waypoint.rotation != null) {
          const delta = Math.atan2(
            Math.sin(waypoint.rotation - rotation),
            Math.cos(waypoint.rotation - rotation)
          )
          rotation += delta * fraction
        } else if (length > 1e-5) rotation = Math.atan2(dx, dz)
        elapsed -= (waypoint.travel_seconds ?? 0) * fraction
        budget -= length * fraction
        if (fraction < 1) break
        if (index === this.waypoints.length - 1) speed = 0
      }
      if (elapsedMs >= MAX_EXTRAPOLATION_MS) speed = 0
    }
    return { position, rotation, speed }
  }

  finish() {
    this.clear(false)
  }

  clear(notify = true) {
    const active = this.active
    if (this.timer !== null) clearTimeout(this.timer)
    this.timer = null
    this.pending = null
    this.directionInput = null
    this.requestId = null
    this.anchor = null
    this.waypoints = []
    this.blockedPose = null
    if (!notify) this.stopId = null
    if (active && notify) {
      this.stopId = this.nextId()
      this.sendStop(this.stopId)
    }
  }
}
