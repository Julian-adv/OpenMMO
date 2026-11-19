import * as THREE from 'three'

class KneeGuardBone extends THREE.Bone {
  private shin?: THREE.Object3D
  readonly rest = new THREE.Quaternion()

  override copy(source: this, recursive = true): this {
    super.copy(source, recursive)
    this.rest.copy(source.rest)
    return this
  }

  override updateMatrix() {
    this.shin ??= this.parent?.children.find(
      (bone) => bone.name === this.name.replace('KneeGuard', 'Leg')
    )
    if (this.shin) {
      this.position.copy(this.shin.position)
      this.quaternion
        .copy(this.rest)
        .slerp(this.shin.quaternion, 0.5)
        .normalize()
    }
    super.updateMatrix()
  }
}

const guardRigs = new WeakMap<THREE.Skeleton, THREE.Skeleton>()

export function kneeGuardSkeleton(
  base: THREE.Skeleton,
  source: THREE.Skeleton
) {
  if (source.bones.length !== base.bones.length + 2) return
  const sides = ['Left', 'Right']
  if (
    sides.some(
      (side, i) =>
        source.bones[base.bones.length + i].name !== side + 'KneeGuard'
    )
  )
    return
  const targets = sides.map((side, i) => {
    const index = base.bones.length + i
    const guard = source.bones[index]
    const shin = base.getBoneByName(side + 'Leg')
    if (
      !shin ||
      guard.parent?.name !== shin.parent?.name ||
      !source.boneInverses[index].equals(
        source.boneInverses[base.bones.indexOf(shin)]
      )
    )
      throw new Error('무릎 보호대의 기준 관절이 다릅니다.')
    return shin
  })
  let rig = guardRigs.get(base)
  if (rig) return rig
  const guards = targets.map((shin) => {
    const parent = shin.parent as THREE.Bone
    const name = shin.name.replace('Leg', 'KneeGuard')
    const existing = parent.children.find((bone) => bone.name === name)
    if (existing instanceof KneeGuardBone) return existing
    const guard = new KneeGuardBone()
    guard.name = name
    const local = base.boneInverses[base.bones.indexOf(parent)]
      .clone()
      .multiply(base.boneInverses[base.bones.indexOf(shin)].clone().invert())
    local.decompose(guard.position, guard.rest, guard.scale)
    guard.rest.normalize()
    parent.add(guard)
    guard.updateWorldMatrix(true, false)
    return guard
  })
  rig = new THREE.Skeleton(
    [...base.bones, ...guards],
    [
      ...base.boneInverses,
      ...targets.map((bone) => base.boneInverses[base.bones.indexOf(bone)]),
    ]
  )
  guardRigs.set(base, rig)
  return rig
}
