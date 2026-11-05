import type * as THREE from 'three'
import { DamageTextEmitter } from './damage-text-pool'
import { takePropTexts } from './prop-text-queue'

const live: { emitter: DamageTextEmitter; at: THREE.Vector3Like }[] = []

export function updatePropTexts(deltaS: number, camera: THREE.Camera) {
  for (const { text, color, at } of takePropTexts()) {
    const emitter = new DamageTextEmitter()
    emitter.spawn(text, color, 0)
    live.push({ emitter, at })
  }
  let kept = 0
  for (const t of live) {
    t.emitter.update(deltaS, t.at.x, t.at.y, t.at.z, camera, {}, 0)
    if (t.emitter.active) live[kept++] = t
  }
  live.length = kept
}
