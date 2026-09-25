import * as THREE from 'three'

export const ENCHANT_WEAPON_RAYS = 48

const axes = new WeakMap<THREE.Object3D, WeaponEffectAxis>()

export interface WeaponEffectAxis {
  center: THREE.Vector3
  direction: THREE.Vector3
  length: number
  width: number
  widthDirection: THREE.Vector3
}

export function getWeaponEffectAxis(weapon: THREE.Object3D): WeaponEffectAxis {
  const cached = axes.get(weapon)
  if (cached) return cached
  weapon.updateWorldMatrix(true, true)
  const inverse = weapon.matrixWorld.clone().invert()
  const transform = new THREE.Matrix4()
  const bounds = new THREE.Box3()
  const point = new THREE.Vector3()
  weapon.traverseVisible((object) => {
    if (!(object instanceof THREE.Mesh)) return
    const positions = object.geometry.getAttribute('position')
    if (!positions) return
    transform.multiplyMatrices(inverse, object.matrixWorld)
    for (let i = 0; i < positions.count; i++) {
      point.fromBufferAttribute(positions, i).applyMatrix4(transform)
      bounds.expandByPoint(point)
    }
  })
  const size = bounds.getSize(new THREE.Vector3())
  const component =
    size.x >= size.y && size.x >= size.z ? 0 : size.y >= size.z ? 1 : 2
  const center = bounds.getCenter(new THREE.Vector3())
  const direction = new THREE.Vector3().setComponent(
    component,
    center.getComponent(component) < 0 ? -1 : 1
  )
  const crossSize = size.clone().setComponent(component, -1)
  const widthComponent =
    crossSize.x >= crossSize.y && crossSize.x >= crossSize.z
      ? 0
      : crossSize.y >= crossSize.z
        ? 1
        : 2
  const axis = {
    center,
    direction,
    length: size.getComponent(component),
    width: Math.max(size.getComponent(widthComponent), 0.01),
    widthDirection: new THREE.Vector3().setComponent(widthComponent, 1),
  }
  axes.set(weapon, axis)
  return axis
}
