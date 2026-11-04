import * as THREE from 'three'

interface WingPivot {
  pivot: THREE.Object3D
  axis: THREE.Vector3
  side: number
}

interface WingMotion {
  pivots: WingPivot[]
  elapsed: number
  angle: number
  lastClip?: THREE.AnimationClip
  lastTime?: number
}

const motions = new WeakMap<THREE.Object3D, WingMotion | null>()
const IDLE_PERIOD = 4.5
const WALK_MOTIONS = /walk|jog|run/
const radians = Math.PI / 180

export function attachValkyrieWings(
  root: THREE.Object3D,
  meshes: THREE.SkinnedMesh[]
): THREE.Mesh[] {
  const wings = meshes.map((source) => {
    const index = source.skeleton.bones.findIndex(
      (bone) => bone.name === 'Spine2'
    )
    if (index < 0) throw new Error('발키리 날개의 등뼈를 찾지 못했습니다.')
    const spine = source.skeleton.bones[index]
    const frame = source.skeleton.boneInverses[index]
      .clone()
      .multiply(source.bindMatrix)
    source.geometry.computeBoundingBox()
    const side =
      source.geometry.boundingBox!.getCenter(new THREE.Vector3()).x > 0 ? 1 : -1
    const pivot = new THREE.Group()
    pivot.name = `valkyrie_wing_pivot_${side}`
    pivot.position.set(side * 0.033, 1.4333, -0.129).applyMatrix4(frame)
    const axis = new THREE.Vector3(0, 1, 0).transformDirection(frame)
    pivot.userData.valkyrie_wing = { axis: axis.toArray(), side }
    const wing = new THREE.Mesh(source.geometry, source.material)
    wing.name = source.name
    wing.userData = { ...source.userData }
    wing.visible = source.visible
    wing.castShadow = source.castShadow
    wing.receiveShadow = source.receiveShadow
    wing.frustumCulled = false
    wing.matrixAutoUpdate = false
    wing.matrix
      .makeTranslation(-pivot.position.x, -pivot.position.y, -pivot.position.z)
      .multiply(frame)
    spine.add(pivot)
    pivot.add(wing)
    source.removeFromParent()
    source.dispose()
    return wing
  })
  motions.delete(root)
  return wings
}

function motionFor(root: THREE.Object3D) {
  let motion = motions.get(root)
  if (motion !== undefined) return motion
  const pivots: WingPivot[] = []
  root.traverse((pivot) => {
    const config = pivot.userData.valkyrie_wing
    if (config)
      pivots.push({
        pivot,
        side: config.side,
        axis: new THREE.Vector3().fromArray(config.axis),
      })
  })
  motion = pivots.length ? { pivots, elapsed: 0, angle: 0 } : null
  motions.set(root, motion)
  return motion
}

export function updateValkyrieWings(
  root: THREE.Object3D,
  delta: number,
  action?: THREE.AnimationAction | null
) {
  const motion = motionFor(root)
  if (!motion) return
  const dt = Number.isFinite(delta) ? Math.max(0, delta) : 0
  motion.elapsed = (motion.elapsed + dt) % IDLE_PERIOD
  const clip = action?.getClip()
  const time = action?.time ?? 0
  let target = 0
  let response = 8
  if (clip && WALK_MOTIONS.test(clip.name) && clip.duration > 0) {
    const beat = 0.5 - 0.5 * Math.cos((time / clip.duration) * Math.PI * 4)
    target = (8 + beat * 32) * radians
    response = 20
  } else if (clip?.name.includes('idle')) {
    const breath =
      0.5 - 0.5 * Math.cos((motion.elapsed / IDLE_PERIOD) * Math.PI * 2)
    target = (4 + breath * 6) * radians
  }
  if (dt > 0)
    motion.angle = THREE.MathUtils.lerp(
      motion.angle,
      target,
      1 - Math.exp(-response * dt)
    )
  else if (clip !== motion.lastClip || time !== motion.lastTime)
    motion.angle = target
  motion.lastClip = clip
  motion.lastTime = time
  for (const { pivot, axis, side } of motion.pivots)
    pivot.quaternion.setFromAxisAngle(axis, motion.angle * side)
}

export function disposeValkyrieWings(root: THREE.Object3D) {
  motions.delete(root)
}
