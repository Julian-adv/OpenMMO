import { expect, test } from 'vitest'
import * as THREE from 'three'
import { femaleBracerMask } from './female-bracer-mask'

test('keeps skin at the cuff edge and wrist while hiding interior protrusions up to 15mm', () => {
  const body = new THREE.BufferGeometry()
  body.setAttribute(
    'position',
    new THREE.Float32BufferAttribute(
      [
        0.115, 0.5, 0, 0.115, 0.51, 0, 0.115, 0.5, 0.005, 0.102, 0.251, 0,
        0.102, 0.252, 0, 0.102, 0.251, 0.005, 0.102, 0.08, 0, 0.102, 0.081, 0,
        0.102, 0.08, 0.005,
      ],
      3
    )
  )
  body.setIndex([0, 1, 2, 3, 4, 5, 6, 7, 8])
  body.setAttribute(
    'skinIndex',
    new THREE.Uint16BufferAttribute(new Uint16Array(36), 4)
  )
  body.setAttribute(
    'skinWeight',
    new THREE.Float32BufferAttribute(
      Array.from({ length: 36 }, (_, i) => (i % 4 === 0 ? 1 : 0)),
      4
    )
  )
  const geometry = new THREE.CylinderGeometry(0.1, 0.1, 0.5, 64, 1, true)
  geometry.translate(0, 0.5, 0)
  const bracer = new THREE.SkinnedMesh(geometry)
  const mask = femaleBracerMask(
    body,
    [bracer],
    [[new THREE.Vector3(0, 1, 0), new THREE.Vector3()]],
    [[0]]
  )
  expect(mask.hide_triangle_indices).toEqual([0])
  geometry.dispose()
  body.dispose()
})
