import * as THREE from 'three'

/** Play `clip` once on `scene` and hold its last frame; `holdAtEnd` starts
 * there. */
export function bindClipOnce(
  scene: THREE.Object3D,
  clip: THREE.AnimationClip,
  holdAtEnd: boolean
): THREE.AnimationMixer {
  const mixer = new THREE.AnimationMixer(scene)
  const action = mixer.clipAction(clip)
  action.loop = THREE.LoopOnce
  action.clampWhenFinished = true
  action.play()
  if (holdAtEnd) mixer.setTime(clip.duration)
  return mixer
}
