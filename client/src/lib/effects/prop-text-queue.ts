import type * as THREE from 'three'

export interface PropText {
  text: string
  color: string
  at: THREE.Vector3Like
}

const queued: PropText[] = []

/** Float `text` over a world point, like a damage number. Kept free of the
 *  label pool so network handlers can call it without a DOM. */
export function showPropText(
  text: string,
  color: string,
  at: THREE.Vector3Like
) {
  queued.push({ text, color, at })
}

export function takePropTexts(): PropText[] {
  return queued.splice(0)
}
