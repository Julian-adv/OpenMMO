import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import { fitFemaleHelmet, type HelmetFits } from './femaleHelmetFit'
import { HAIR_FIT_DEFAULT } from './femaleHairFit'

const fits: HelmetFits = {
  ceremony_helmet: { ponytail: { ...HAIR_FIT_DEFAULT, scale: 110 } },
}
const helmet = () =>
  new THREE.Mesh(new THREE.BoxGeometry(0.2, 0.2, 0.2).translate(0, 1.7, 0))
const width = (mesh: THREE.Mesh) => {
  mesh.geometry.computeBoundingBox()
  return mesh.geometry.boundingBox!.getSize(new THREE.Vector3()).x
}

describe('fitFemaleHelmet', () => {
  it('scales the helmet about its centre for the saved hair only', () => {
    const mesh = helmet()
    const source = mesh.geometry
    fitFemaleHelmet(mesh, 'ceremony_helmet', 'ranger_hair', fits)
    expect(width(mesh)).toBeCloseTo(0.22)
    expect(
      mesh.geometry.boundingBox!.getCenter(new THREE.Vector3()).y
    ).toBeCloseTo(1.7)
    fitFemaleHelmet(mesh, 'ceremony_helmet', 'hair', fits)
    expect(mesh.geometry).toBe(source)
  })

  it('reuses one fitted geometry and leaves other helmets alone', () => {
    const mesh = helmet()
    fitFemaleHelmet(mesh, 'ceremony_helmet', 'ranger_hair', fits)
    const fitted = mesh.geometry
    fitFemaleHelmet(mesh, 'ceremony_helmet', null, fits)
    fitFemaleHelmet(mesh, 'ceremony_helmet', 'ranger_hair', fits)
    expect(mesh.geometry).toBe(fitted)
    const other = helmet()
    const source = other.geometry
    fitFemaleHelmet(other, 'bard_helmet', 'ranger_hair', fits)
    expect(other.geometry).toBe(source)
  })
})
