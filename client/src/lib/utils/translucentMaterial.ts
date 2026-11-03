import * as THREE from 'three'

/** A see-through copy of `material` that does not hide what is behind it. */
export function translucentClone(
  material: THREE.Material,
  opacity: number
): THREE.Material {
  const clone = material.clone()
  clone.transparent = true
  clone.opacity = opacity
  clone.depthWrite = false
  return clone
}
