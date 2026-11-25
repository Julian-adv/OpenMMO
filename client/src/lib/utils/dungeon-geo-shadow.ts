import * as THREE from 'three'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import { SHADOW_CASTER_LAYER } from './pointShadow'

export const WALL_SHADOW_CASTER_NAME = 'wallShadowCaster'

const material = new THREE.MeshBasicMaterial()

/** One shadow-only mesh of wall-run boxes. Runs fade and hide on their own, so
 *  they cast nothing; this keeps their shadow in a single draw per cube face.
 *  The shadow pass draws back faces, and each box spans its run's full relief,
 *  so lit wall faces never shadow themselves. */
export function buildWallShadowCaster(boxes: THREE.Box3[]): THREE.Mesh | null {
  if (boxes.length === 0) return null
  const size = new THREE.Vector3()
  const center = new THREE.Vector3()
  const geos = boxes.map((box) => {
    box.getSize(size)
    box.getCenter(center)
    const geo = new THREE.BoxGeometry(size.x, size.y, size.z)
    geo.deleteAttribute('normal')
    geo.deleteAttribute('uv')
    return geo.translate(center.x, center.y, center.z)
  })
  const merged = mergeGeometries(geos)
  for (const geo of geos) geo.dispose()
  const mesh = new THREE.Mesh(merged, material)
  mesh.name = WALL_SHADOW_CASTER_NAME
  mesh.layers.set(SHADOW_CASTER_LAYER)
  mesh.castShadow = true
  mesh.raycast = () => {}
  return mesh
}
