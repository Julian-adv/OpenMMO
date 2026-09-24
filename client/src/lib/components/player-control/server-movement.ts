import type {
  MoveGoal,
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
  private requestId: number | null = null
  private stopId: number | null = null
  private waypoints: MoveWaypoint[] = []
  private anchor: MoveProgress | null = null
  private anchorAt = 0
  private nextSendAt = 0
  private pending: MoveGoal | null = null
  private timer: ReturnType<typeof setTimeout> | null = null

  constructor(
    private readonly nextId: () => number,
    private readonly sendGoal: (goal: MoveGoal) => void,
    private readonly sendStop: (requestId: number) => void
  ) {}

  get active() {
    return this.requestId !== null
  }

  request(x: number, z: number, sprinting: boolean) {
    if (!Number.isFinite(x) || !Number.isFinite(z)) return
    this.stopId = null
    this.requestId = this.nextId()
    this.anchor = null
    this.waypoints = []
    this.pending = {
      request_id: this.requestId,
      x: wrapWorldX(x),
      z,
      sprinting,
    }
    const delay = this.nextSendAt - performance.now()
    if (delay <= 0) this.flush()
    else if (this.timer === null)
      this.timer = setTimeout(() => this.flush(), delay)
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
    if (path.request_id !== this.requestId) return false
    this.waypoints = path.waypoints
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
    this.anchorAt = performance.now()
    return true
  }

  sample(blocked: (from: Position, to: Position) => boolean): Pose | null {
    const anchor = this.anchor
    if (!anchor) return null
    let position = { ...anchor.position }
    let rotation = anchor.rotation
    let speed = this.waypoints.length > anchor.next_waypoint ? anchor.speed : 0
    let budget =
      (speed *
        Math.min(performance.now() - this.anchorAt, MAX_EXTRAPOLATION_MS)) /
      1000
    if (anchor.status === 'moving') {
      for (
        let index = anchor.next_waypoint;
        index < this.waypoints.length;
        index++
      ) {
        const target = this.waypoints[index].position
        const dx = shortestWrappedDeltaX(position.x, target.x)
        const dz = target.z - position.z
        const length = Math.hypot(dx, dz)
        const fraction = length < 1e-5 ? 1 : Math.min(budget / length, 1)
        const next = {
          x: wrapWorldX(position.x + dx * fraction),
          y: position.y + (target.y - position.y) * fraction,
          z: position.z + dz * fraction,
        }
        if (blocked(position, next)) {
          speed = 0
          break
        }
        position = next
        if (length > 1e-5) rotation = Math.atan2(dx, dz)
        budget -= length * fraction
        if (fraction < 1) break
        if (index === this.waypoints.length - 1) speed = 0
      }
      if (performance.now() - this.anchorAt >= MAX_EXTRAPOLATION_MS) speed = 0
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
    this.requestId = null
    this.anchor = null
    this.waypoints = []
    if (!notify) this.stopId = null
    if (active && notify) {
      this.stopId = this.nextId()
      this.sendStop(this.stopId)
    }
  }
}
