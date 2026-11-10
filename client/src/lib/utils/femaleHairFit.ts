import * as THREE from 'three'
import saved from '../data/femaleHairFits.json'

export const HAIR_FIT_DEFAULT = {
  'move-x': 0,
  'move-y': 0,
  'move-z': 0,
  scale: 100,
  'scale-x': 100,
  'scale-y': 100,
  'scale-z': 100,
  'rotate-x': 0,
  'rotate-y': 0,
  'rotate-z': 0,
}
export type HairFit = typeof HAIR_FIT_DEFAULT
export type HairFitFace = 'default' | 'custom' | 'ranger' | 'chic'
export type HairFitStyle = 'bob' | 'long' | 'ponytail'
export type HairFits = Record<
  HairFitFace,
  Partial<Record<HairFitStyle, HairFit>>
>
const styles: Record<string, HairFitStyle> = {
  hair: 'bob',
  barbarian_hair: 'bob',
  valkyrie_hair: 'long',
  ranger_hair: 'ponytail',
}
const geometries = new WeakMap<
  THREE.BufferGeometry,
  Map<string, THREE.BufferGeometry>
>()

export function femaleHairPivot(mesh: THREE.SkinnedMesh): THREE.Vector3 {
  const head = mesh.skeleton.bones.findIndex((bone) => bone.name === 'Head')
  if (head < 0) throw new Error('헤어의 머리 본을 찾을 수 없습니다.')
  return new THREE.Vector3()
    .setFromMatrixPosition(mesh.skeleton.boneInverses[head].clone().invert())
    .applyMatrix4(mesh.bindMatrixInverse)
}

export function femaleHairFitMatrix(fit: HairFit, pivot: THREE.Vector3) {
  const rotation = new THREE.Euler(
    ...((['x', 'y', 'z'] as const).map((axis) =>
      THREE.MathUtils.degToRad(fit[`rotate-${axis}`])
    ) as [number, number, number])
  )
  return new THREE.Matrix4()
    .makeTranslation(
      pivot.x + fit['move-x'] / 100,
      pivot.y + fit['move-y'] / 100,
      pivot.z + fit['move-z'] / 100
    )
    .multiply(new THREE.Matrix4().makeRotationFromEuler(rotation))
    .multiply(
      new THREE.Matrix4().makeScale(
        (fit.scale * fit['scale-x']) / 10000,
        (fit.scale * fit['scale-y']) / 10000,
        (fit.scale * fit['scale-z']) / 10000
      )
    )
    .multiply(new THREE.Matrix4().makeTranslation(-pivot.x, -pivot.y, -pivot.z))
}

export function fitFemaleHair(
  mesh: THREE.SkinnedMesh,
  face: string,
  part: string
) {
  const style = styles[part]
  if (!style) return
  const fit = (saved as HairFits)[
    face in saved ? (face as HairFitFace) : 'default'
  ][style]
  if (
    !fit ||
    Object.entries(HAIR_FIT_DEFAULT).every(
      ([key, value]) => fit[key as keyof HairFit] === value
    )
  )
    return
  const source = mesh.geometry
  const pivot = femaleHairPivot(mesh)
  const key = JSON.stringify([fit, pivot.toArray()])
  let cache = geometries.get(source)
  if (!cache) geometries.set(source, (cache = new Map()))
  let geometry = cache.get(key)
  if (!geometry) {
    geometry = source.clone().applyMatrix4(femaleHairFitMatrix(fit, pivot))
    cache.set(key, geometry)
  }
  mesh.geometry = geometry
}
