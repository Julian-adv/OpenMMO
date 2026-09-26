import type {
  MoveGoal,
  MoveDirection,
  MovePath,
  MoveProgress,
  MoveWaypoint,
  Position,
} from '../../network/networkTypes'
import { shortestWrappedDeltaX, wrapWorldX } from '../../terrain/world-wrap'

const SEND_INTERVAL_MS = 200
const MAX_EXTRAPOLATION_MS = 500
const STOP_BLEND_MS = 120

type Pose = {
  position: Position
  rotation: number
  speed: number
}

type MovementUpdate = {
  progress: MoveProgress
  waypoints: MoveWaypoint[]
  at: number
}

export class ServerMovement {
  private directionInput: MoveDirection | null = null
  private requestId: number | null = null
  private stopId: number | null = null
  private lastPose: Pose | null = null
  private stopBlend: { from: Pose; target: Pose; at: number } | null = null
  private waypoints: MoveWaypoint[] = []
  private anchor: MoveProgress | null = null
  private anchorAt = 0
  private queuedUpdates: MovementUpdate[] = []
  private sentGoalIds: number[] = []
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

  get stopping() {
    return (
      this.stopBlend !== null ||
      (this.directionInput !== null && this.stopId !== null)
    )
  }

  get searchElapsedMs(): number | null {
    return this.anchor?.status === 'searching'
      ? performance.now() - this.anchorAt
      : null
  }

  isCurrentRequest(requestId: number) {
    return requestId === this.requestId
  }

  request(x: number, z: number, sprinting: boolean, stopAtEntrance = false) {
    if (!Number.isFinite(x) || !Number.isFinite(z)) return
    if (this.directionInput || this.stopping || !this.active) this.clear(false)
    this.requestId = this.nextId()
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
      this.stopping ||
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

  stopDirection() {
    if (!this.directionInput || this.stopping) return
    this.stopId = this.nextId()
    this.sendStop(this.stopId)
  }

  private flush() {
    if (this.timer !== null) clearTimeout(this.timer)
    this.timer = null
    if (!this.pending) return
    this.sentGoalIds.push(this.pending.request_id)
    this.sendGoal(this.pending)
    this.pending = null
    this.nextSendAt = performance.now() + SEND_INTERVAL_MS
  }

  acceptPath(path: MovePath): boolean {
    return this.acceptUpdate(
      { ...path, next_waypoint: 0, status: 'moving' },
      path.waypoints
    )
  }

  private acceptUpdate(progress: MoveProgress, waypoints: MoveWaypoint[]) {
    const goalIndex = this.sentGoalIds.indexOf(progress.request_id)
    if (
      (this.directionInput
        ? !this.isCurrentRequest(progress.request_id)
        : goalIndex < 0) ||
      progress.server_time_ms < this.latestServerTime
    )
      return false
    if (goalIndex >= 0) this.sentGoalIds.splice(0, goalIndex)
    const now = performance.now()
    // Retargets and progress share the approved path's playback clock.
    const at = this.anchor
      ? this.anchorAt + (progress.server_time_ms - this.anchor.server_time_ms)
      : now
    const update = { progress, waypoints, at }
    if (at > now) this.queuedUpdates.push(update)
    else {
      this.queuedUpdates = []
      this.applyUpdate(update)
    }
    return true
  }

  private get latestServerTime() {
    return (
      this.queuedUpdates.at(-1)?.progress.server_time_ms ??
      this.anchor?.server_time_ms ??
      0
    )
  }

  private applyUpdate({ progress, waypoints, at }: MovementUpdate) {
    this.waypoints = waypoints
    this.blockedPose = null
    this.anchor = progress
    this.anchorAt = at
  }

  acceptStopped(
    progress: MoveProgress,
    displayed = this.lastPose,
    blendPosition = false
  ): boolean {
    if (progress.request_id !== this.stopId || progress.status !== 'stopped')
      return false
    this.stopId = null
    if (this.directionInput || blendPosition) {
      this.clear(false)
      if (displayed) {
        this.stopBlend = {
          from: { ...displayed, position: { ...displayed.position } },
          target: {
            position: { ...progress.position },
            rotation: progress.rotation,
            speed: 0,
          },
          at: performance.now(),
        }
        this.lastPose = this.stopBlend.from
      }
    }
    return true
  }

  acceptProgress(progress: MoveProgress): boolean {
    const latest = this.queuedUpdates.at(-1)
    const previous = latest?.progress ?? this.anchor
    const waypoints =
      progress.status === 'moving' &&
      progress.request_id === previous?.request_id
        ? (latest?.waypoints ?? this.waypoints)
        : []
    return this.acceptUpdate(progress, waypoints)
  }

  sample(blocked: (from: Position, to: Position) => boolean): Pose | null {
    const pose = this.stopBlend
      ? this.sampleStop(blocked)
      : this.samplePath(blocked)
    if (pose) this.lastPose = pose
    return pose
  }

  private sampleStop(blocked: (from: Position, to: Position) => boolean): Pose {
    const { from, target, at } = this.stopBlend!
    const elapsed = Math.min((performance.now() - at) / STOP_BLEND_MS, 1)
    const fraction = 1 - (1 - elapsed) ** 2
    const dx = shortestWrappedDeltaX(from.position.x, target.position.x)
    const rotationDelta = Math.atan2(
      Math.sin(target.rotation - from.rotation),
      Math.cos(target.rotation - from.rotation)
    )
    const position = {
      x: wrapWorldX(from.position.x + dx * fraction),
      y: from.position.y + (target.position.y - from.position.y) * fraction,
      z: from.position.z + (target.position.z - from.position.z) * fraction,
    }
    const settled =
      Math.hypot(
        dx,
        target.position.y - from.position.y,
        target.position.z - from.position.z
      ) < 1e-5 && Math.abs(rotationDelta) < 1e-5
    if (
      elapsed >= 1 ||
      settled ||
      blocked(this.lastPose?.position ?? from.position, position)
    ) {
      this.stopBlend = null
      return target
    }
    return {
      position,
      rotation: from.rotation + rotationDelta * fraction,
      speed: from.speed * (1 - elapsed),
    }
  }

  private samplePath(
    blocked: (from: Position, to: Position) => boolean
  ): Pose | null {
    const now = performance.now()
    while (this.queuedUpdates.length && this.queuedUpdates[0].at <= now) {
      this.applyUpdate(this.queuedUpdates.shift()!)
    }
    if (this.blockedPose) return this.blockedPose
    const anchor = this.anchor
    if (!anchor) return null
    let position = { ...anchor.position }
    let rotation = anchor.rotation
    let speed = this.waypoints.length > anchor.next_waypoint ? anchor.speed : 0
    const elapsedMs = now - this.anchorAt
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
    this.lastPose = null
    this.stopBlend = null
    this.requestId = null
    this.anchor = null
    this.queuedUpdates = []
    this.sentGoalIds = []
    this.waypoints = []
    this.blockedPose = null
    if (!notify) this.stopId = null
    if (active && notify) {
      this.stopId = this.nextId()
      this.sendStop(this.stopId)
    }
  }
}
