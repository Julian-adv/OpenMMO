import * as THREE from 'three'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import { getWeaponEffectAxis } from './weaponEffectAxis'

export function createWeaponGlowGeometry(
  weapon: THREE.Object3D
): THREE.BufferGeometry {
  const axis = getWeaponEffectAxis(weapon)
  weapon.updateWorldMatrix(true, true)
  const inverse = weapon.matrixWorld.clone().invert()
  const transform = new THREE.Matrix4()
  const point = new THREE.Vector3()
  const parts: THREE.BufferGeometry[] = []
  weapon.traverseVisible((object) => {
    if (!(object instanceof THREE.Mesh)) return
    const position = object.geometry.getAttribute('position')
    if (!position) return
    const source = new THREE.BufferGeometry().setAttribute(
      'position',
      position.clone()
    )
    source.setIndex(object.geometry.index)
    const geometry = source.index ? source.toNonIndexed() : source
    if (geometry !== source) source.dispose()
    transform.multiplyMatrices(inverse, object.matrixWorld)
    geometry.applyMatrix4(transform)
    geometry.computeVertexNormals()
    const positions = geometry.getAttribute('position')
    const uv = new THREE.Float32BufferAttribute(
      new Float32Array(positions.count * 2),
      2
    )
    for (let i = 0; i < positions.count; i++) {
      point.fromBufferAttribute(positions, i).sub(axis.center)
      uv.setXY(
        i,
        point.dot(axis.widthDirection) / axis.width + 0.5,
        point.dot(axis.direction) / Math.max(axis.length, 0.01) + 0.5
      )
    }
    geometry.setAttribute('uv', uv)
    parts.push(geometry)
  })
  const merged = parts.length ? mergeGeometries(parts) : null
  parts.forEach((part) => part.dispose())
  return merged ?? new THREE.BufferGeometry()
}
