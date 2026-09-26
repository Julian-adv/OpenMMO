import * as THREE from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { angleDelta } from './horseMovement'
import { shortestWrappedDeltaX } from '../terrain/world-wrap'

export const HORSE_MODEL_PATH = '/models/mounts/horse.glb'
export const RIDING_ANIMATION_PATH = '/models/animations/riding.glb'
const RUN_STRIDE_DURATION = 20 / 30
const RIDER_IDLE_YAW_LIMIT = Math.PI / 6
export const TURN_LOOP_START = 0.25
const TURN_LOOP_END = 0.65

export class HorseMount {
  readonly root: THREE.Object3D
  readonly seat: THREE.Object3D
  riderHipLift = 0
  riderHandLift = 0
  riderIdleWeight = 0
  riderBaseOffsetY = 0
  riderFacingYaw = 0
  private readonly head: THREE.Object3D | undefined
  private readonly headForward = new THREE.Vector3()
  private readonly bodyForward = new THREE.Vector3()
  private readonly runSeatHeight: number
  private readonly seatPosition = new THREE.Vector3()
  private readonly mixer: THREE.AnimationMixer
  private readonly actions = new Map<string, THREE.AnimationAction>()
  private readonly turnRepeats = new Map<string, THREE.AnimationAction>()
  private current: THREE.AnimationAction | null = null
  private previousRotation: number | null = null
  private turnName: string | null = null
  private previousPosition: { x: number; z: number } | null = null
  private reversing = false

  constructor(gltf: GLTF) {
    this.root = clone(gltf.scene)
    this.seat = this.root.getObjectByName('RideSeat') ?? this.root
    this.head = this.root.getObjectByName('Head')
    this.mixer = new THREE.AnimationMixer(this.root)
    this.root.traverse((node) => {
      if (node instanceof THREE.Mesh) {
        node.castShadow = true
        node.receiveShadow = true
      }
    })
    for (const clip of gltf.animations) {
      this.actions.set(clip.name, this.mixer.clipAction(clip))
    }
    let height = 0
    const run = this.actions.get('run')
    if (run) {
      run.play()
      for (let i = 0; i < 8; i++) {
        this.mixer.setTime((i / 8) * RUN_STRIDE_DURATION)
        this.seat.getWorldPosition(this.seatPosition)
        height += this.root.worldToLocal(this.seatPosition).y / 8
      }
      run.stop()
      this.mixer.setTime(0)
    }
    this.runSeatHeight = height
  }

  update(
    dt: number,
    speed: number,
    rotation = 0,
    position?: { x: number; z: number }
  ) {
    if (position && this.previousPosition) {
      const dx = shortestWrappedDeltaX(this.previousPosition.x, position.x)
      const dz = position.z - this.previousPosition.z
      const distance = Math.hypot(dx, dz)
      if (distance > 0.00001) {
        this.reversing =
          distance < 1 &&
          dx * Math.sin(rotation) + dz * Math.cos(rotation) < -distance * 0.5
      }
    }
    if (position) {
      this.previousPosition ??= { x: 0, z: 0 }
      this.previousPosition.x = position.x
      this.previousPosition.z = position.z
    } else {
      this.previousPosition = null
    }
    if (speed < 0.1) this.reversing = false
    const yaw =
      this.previousRotation === null
        ? 0
        : angleDelta(this.previousRotation, rotation)
    this.previousRotation = rotation
    const angularSpeed = dt > 0 ? Math.abs(yaw) / dt : 0
    const turning = angularSpeed > 0.1 && speed < 3 && !this.reversing
    if (turning) {
      const side = yaw < 0 ? 'right' : 'left'
      if (!this.turnName?.startsWith(`turn_${side}_`)) {
        this.turnName = `turn_${side}_${angularSpeed > 3.2 ? 180 : 90}`
      }
    } else {
      this.turnName = null
    }
    const name =
      this.turnName ??
      (speed < 0.1 ? 'idle' : this.reversing || speed < 3 ? 'walk' : 'run')
    const continuing = this.current?.getClip().name === name
    let next = continuing ? this.current : this.actions.get(name)
    let repeat = false
    if (
      this.turnName &&
      continuing &&
      next &&
      next.time >= next.getClip().duration * TURN_LOOP_END
    ) {
      repeat = true
      let alternate = this.turnRepeats.get(name)
      if (!alternate) {
        const clip = next.getClip()
        alternate = this.mixer.clipAction(
          new THREE.AnimationClip(
            clip.name,
            clip.duration,
            clip.tracks,
            clip.blendMode
          )
        )
        this.turnRepeats.set(name, alternate)
      }
      next = next === alternate ? this.actions.get(name) : alternate
    }
    if (next && next !== this.current) {
      next.reset().setEffectiveWeight(1).play()
      // Start mid-turn so the head leads the body from the first frame.
      if (this.turnName) next.time = next.getClip().duration * TURN_LOOP_START
      else if (this.reversing) next.time = next.getClip().duration
      next.setLoop(this.turnName ? THREE.LoopOnce : THREE.LoopRepeat, Infinity)
      next.clampWhenFinished = this.turnName !== null
      if (this.current)
        next.crossFadeFrom(this.current, repeat ? 0.15 : 0.2, false)
      this.current = next
    }
    if (this.current) {
      this.current.timeScale = this.turnName
        ? (this.current.getClip().duration *
            (TURN_LOOP_END - TURN_LOOP_START) *
            angularSpeed) /
          (name.endsWith('180') ? Math.PI : Math.PI / 2)
        : name === 'idle'
          ? 1
          : ((this.reversing ? -1 : 1) * speed) / (name === 'walk' ? 2 : 8)
    }
    this.mixer.update(dt)
    let riderYaw = 0
    if (this.head) {
      this.head.updateWorldMatrix(true, false)
      this.headForward.set(0, 0, 1).transformDirection(this.head.matrixWorld)
      this.bodyForward.set(0, 0, 1).transformDirection(this.root.matrixWorld)
      if (Math.hypot(this.headForward.x, this.headForward.z) > 0.0001) {
        riderYaw = angleDelta(
          Math.atan2(this.bodyForward.x, this.bodyForward.z),
          Math.atan2(this.headForward.x, this.headForward.z)
        )
      }
    }
    if (!turning) {
      riderYaw = THREE.MathUtils.clamp(
        riderYaw,
        -RIDER_IDLE_YAW_LIMIT,
        RIDER_IDLE_YAW_LIMIT
      )
    }
    this.riderFacingYaw = THREE.MathUtils.damp(
      this.riderFacingYaw,
      riderYaw,
      turning && Math.abs(riderYaw) > Math.abs(this.riderFacingYaw) ? 12 : 6,
      Math.max(0, dt)
    )
    const run = this.actions.get('run')
    const weight = run?.isRunning() ? run.getEffectiveWeight() : 0
    const runPhase = ((run?.time ?? 0) * Math.PI * 2) / RUN_STRIDE_DURATION
    this.seat.getWorldPosition(this.seatPosition)
    const seatY = this.root.worldToLocal(this.seatPosition).y
    this.riderBaseOffsetY = (this.runSeatHeight - 0.02 - seatY) * weight
    const bounce = 0.045 * (1 - Math.cos(runPhase)) * weight
    this.riderHipLift = Math.max(0, bounce, -this.riderBaseOffsetY)
    const idle = this.actions.get('idle')
    const idleWeight = idle?.isRunning() ? idle.getEffectiveWeight() : 0
    this.riderIdleWeight = idleWeight
    this.riderHandLift =
      -0.2 * idleWeight +
      Math.sin((this.mixer.time * Math.PI) / 2) * 0.004 +
      Math.sin(runPhase - 0.4) * 0.015 * weight
  }

  dispose() {
    this.mixer.stopAllAction()
    this.mixer.uncacheRoot(this.root)
  }
}
