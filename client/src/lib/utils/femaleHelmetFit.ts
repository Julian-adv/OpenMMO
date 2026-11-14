import * as THREE from 'three'
import saved from '../data/femaleHelmetFits.json'
import {
  femaleHairFitMatrix,
  femaleHairStyle,
  type HairFit,
  type HairFitStyle,
} from './femaleHairFit'

// Runtime helmet part → hair style → adjustment on top of the fitted helmet, saved from the female preview.
export type HelmetFits = Record<string, Partial<Record<HairFitStyle, HairFit>>>

const sources = new WeakMap<THREE.Mesh, THREE.BufferGeometry>()
const fitted = new WeakMap<
  THREE.BufferGeometry,
  Map<string, THREE.BufferGeometry>
>()

/** Wears the helmet as adjusted for the shown hair; without a saved adjustment it keeps its fitted shape. */
export function fitFemaleHelmet(
  mesh: THREE.Mesh,
  helmet: string,
  hairPart: string | null | undefined,
  fits: HelmetFits = saved
) {
  const source = sources.get(mesh) ?? mesh.geometry
  sources.set(mesh, source)
  const style = hairPart ? femaleHairStyle(hairPart) : undefined
  const fit = style && fits[helmet]?.[style]
  if (!fit) {
    mesh.geometry = source
    return
  }
  let cache = fitted.get(source)
  if (!cache) fitted.set(source, (cache = new Map()))
  const key = JSON.stringify(fit)
  let geometry = cache.get(key)
  if (!geometry) {
    if (!source.boundingBox) source.computeBoundingBox()
    const centre = source.boundingBox!.getCenter(new THREE.Vector3())
    geometry = source.clone().applyMatrix4(femaleHairFitMatrix(fit, centre))
    cache.set(key, geometry)
  }
  mesh.geometry = geometry
}
