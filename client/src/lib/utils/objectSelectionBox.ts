import * as THREE from 'three'

export function createSelectionBox(center: THREE.Vector3, size: THREE.Vector3) {
  const box = new THREE.BoxGeometry(size.x, size.y, size.z)
  const geometry = new THREE.EdgesGeometry(box)
  box.dispose()
  const material = new THREE.LineBasicMaterial({
    color: 0x44ccff,
    depthTest: false,
    transparent: true,
    opacity: 0.9,
  })
  const lines = new THREE.LineSegments(geometry, material)
  lines.position.copy(center)
  lines.renderOrder = 999
  lines.raycast = () => {}
  return lines
}
