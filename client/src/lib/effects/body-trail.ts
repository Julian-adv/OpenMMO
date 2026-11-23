import * as THREE from 'three'

const STEP = 0.3
const REVERSE = 0.8
const TELEPORT = 6
const LEAD_SECONDS = 0.25
const TURN_RATE = Math.PI
const LOOK_RATE = Math.PI * 1.5
const MAX_BEND = THREE.MathUtils.degToRad(45)
const MAX_LOOK = THREE.MathUtils.degToRad(80)
const REST_TURN_RATE = Math.PI / 2
const LOOK_SHARES: [string, number][] = [
  ['Neck1', 0.4],
  ['Neck2', 0.35],
  ['Head', 0.25],
]
const UP = new THREE.Vector3(0, 1, 0)
const axis = new THREE.Vector3()
const parentQ = new THREE.Quaternion()
const turn = new THREE.Quaternion()

function wrap(angle: number) {
  return Math.atan2(Math.sin(angle), Math.cos(angle))
}

type Point = { x: number; z: number }

/** Steer the animated bones along the neck's recent path after the mixer runs. */
export class BodyTrail {
  private readonly tail: THREE.Bone[]
  private readonly look: [THREE.Bone, number][] = []
  private readonly spans: { from: number; length: number; yaw: number }[] = []
  private readonly reach: number
  /** The neck's path, newest first. */
  private path: Point[] = []
  private heading = 0
  private shown = 0
  private lookAngle = 0
  private dt = 0
  private lead: Point | undefined
  private readonly applied = new Map<
    THREE.Bone,
    { clip: THREE.Quaternion; out: THREE.Quaternion }
  >()
  private last = 0

  constructor(root: THREE.Object3D) {
    const tail: THREE.Bone[] = []
    root.traverse((o) => {
      if (o instanceof THREE.Bone && /^Tail\d+$/.test(o.name)) tail.push(o)
    })
    this.tail = tail.sort(
      (a, b) => Number(a.name.slice(4)) - Number(b.name.slice(4))
    )
    // Rest-pose distances along the ground, measured in the model's space.
    root.updateMatrixWorld(true)
    const at = (bone: THREE.Object3D) =>
      root.worldToLocal(bone.getWorldPosition(new THREE.Vector3()))
    const heads = this.tail.map(at)
    let from = Math.hypot(heads[0]?.x ?? 0, heads[0]?.z ?? 0)
    heads.forEach((head, i) => {
      const next = heads[i + 1]
      const length = next
        ? Math.hypot(next.x - head.x, next.z - head.z)
        : (this.spans[i - 1]?.length ?? 0.3)
      this.spans.push({ from, length, yaw: 0 })
      from += length
    })
    this.reach = from + 0.5
    for (const [name, share] of LOOK_SHARES) {
      const bone = root.getObjectByName(name)
      if (bone instanceof THREE.Bone) this.look.push([bone, share])
    }
  }

  /** Record where the neck is and return the heading the root should show. */
  track(neckX: number, neckZ: number, facing: number, now: number): number {
    this.dt = Math.max(0, now - this.last) / 1000
    this.last = now
    const lead = this.lead
    if (!lead || Math.hypot(neckX - lead.x, neckZ - lead.z) > TELEPORT) {
      this.lead = { x: neckX, z: neckZ }
      this.heading = this.shown = facing
      this.lookAngle = this.dt = 0
      for (const span of this.spans) span.yaw = facing
      this.path = [
        { x: neckX, z: neckZ },
        {
          x: neckX - Math.sin(facing) * this.reach,
          z: neckZ - Math.cos(facing) * this.reach,
        },
      ]
      return this.shown
    }
    const pull = 1 - Math.exp(-this.dt / LEAD_SECONDS)
    lead.x += (neckX - lead.x) * pull
    lead.z += (neckZ - lead.z) * pull
    const [head] = this.path
    const dx = lead.x - head.x
    const dz = lead.z - head.z
    const backward =
      Math.abs(wrap(Math.atan2(dx, dz) - this.heading)) > (Math.PI * 2) / 3
    if (Math.hypot(dx, dz) >= (backward ? REVERSE : STEP)) {
      this.path.unshift({ x: lead.x, z: lead.z })
      this.heading = Math.atan2(dx, dz)
      this.trim()
    } else {
      // At rest the body only swings round when the head can't reach.
      const gap = wrap(facing - this.heading)
      const beyond = Math.abs(gap) - MAX_LOOK
      if (beyond > 0)
        this.swing(Math.sign(gap) * Math.min(beyond, REST_TURN_RATE * this.dt))
    }
    const limit = TURN_RATE * this.dt
    this.shown = wrap(
      this.shown +
        THREE.MathUtils.clamp(wrap(this.heading - this.shown), -limit, limit)
    )
    return this.shown
  }

  /** Lay the tail along the path behind a root turned to `rootYaw`, and turn
   *  the head toward `facing`. */
  pose(rootYaw: number, facing: number) {
    this.restore()
    const limit = TURN_RATE * this.dt
    let parentYaw = rootYaw
    let near = this.at(this.spans[0]?.from ?? 0)
    this.tail.forEach((bone, i) => {
      const span = this.spans[i]
      const far = this.at(span.from + span.length)
      const target = Math.atan2(near.x - far.x, near.z - far.z)
      const yaw =
        span.yaw + THREE.MathUtils.clamp(wrap(target - span.yaw), -limit, limit)
      const step = THREE.MathUtils.clamp(
        wrap(yaw - parentYaw),
        -MAX_BEND,
        MAX_BEND
      )
      span.yaw = parentYaw = wrap(parentYaw + step)
      near = far
      this.turn(bone, step)
    })
    const look = THREE.MathUtils.clamp(
      wrap(facing - rootYaw),
      -MAX_LOOK,
      MAX_LOOK
    )
    const lookStep = LOOK_RATE * this.dt
    this.lookAngle += THREE.MathUtils.clamp(
      look - this.lookAngle,
      -lookStep,
      lookStep
    )
    for (const [bone, share] of this.look)
      this.turn(bone, this.lookAngle * share)
  }

  /** Turn the heading and the path behind the neck about the neck. */
  private swing(angle: number) {
    const [neck] = this.path
    const c = Math.cos(angle)
    const s = Math.sin(angle)
    this.heading = wrap(this.heading + angle)
    this.path = this.path.map((p) => {
      const dx = p.x - neck.x
      const dz = p.z - neck.z
      return { x: neck.x + dx * c + dz * s, z: neck.z - dx * s + dz * c }
    })
  }

  private turn(bone: THREE.Bone, angle: number) {
    let record = this.applied.get(bone)
    if (!record) {
      record = { clip: new THREE.Quaternion(), out: new THREE.Quaternion() }
      this.applied.set(bone, record)
    }
    record.clip.copy(bone.quaternion)
    if (Math.abs(angle) >= 1e-4 && bone.parent) {
      bone.parent.getWorldQuaternion(parentQ).invert()
      axis.copy(UP).applyQuaternion(parentQ)
      bone.quaternion.premultiply(turn.setFromAxisAngle(axis, angle))
    }
    record.out.copy(bone.quaternion)
  }

  /** A clip that has stopped no longer rewrites its bones each frame, so
   *  undo last frame's turn rather than stacking another on it. */
  private restore() {
    for (const [bone, record] of this.applied) {
      if (bone.quaternion.equals(record.out)) bone.quaternion.copy(record.clip)
    }
  }

  /** The point `distance` back along the path, straight past its end. */
  private at(distance: number): Point {
    let left = distance
    for (let i = 1; i < this.path.length; i++) {
      const a = this.path[i - 1]
      const b = this.path[i]
      const length = Math.hypot(b.x - a.x, b.z - a.z)
      if (length >= left && length > 0) {
        const t = left / length
        return { x: a.x + (b.x - a.x) * t, z: a.z + (b.z - a.z) * t }
      }
      left -= length
    }
    const last = this.path[this.path.length - 1]
    const before = this.path[this.path.length - 2] ?? last
    const dx = last.x - before.x
    const dz = last.z - before.z
    const length = Math.hypot(dx, dz)
    if (length === 0) {
      return {
        x: last.x - Math.sin(this.heading) * left,
        z: last.z - Math.cos(this.heading) * left,
      }
    }
    return {
      x: last.x + (dx / length) * left,
      z: last.z + (dz / length) * left,
    }
  }

  private trim() {
    let total = 0
    for (let i = 1; i < this.path.length; i++) {
      const a = this.path[i - 1]
      const b = this.path[i]
      total += Math.hypot(b.x - a.x, b.z - a.z)
      if (total > this.reach) {
        this.path.length = i + 1
        return
      }
    }
  }
}
