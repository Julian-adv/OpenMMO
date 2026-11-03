import * as THREE from 'three'
import {
  StandardNodeLibrary,
  type MeshStandardNodeMaterial,
} from 'three/webgpu'
import {
  attribute,
  materialColor,
  materialMetalness,
  materialRoughness,
  max,
  min,
  mix,
  positionGeometry,
  smoothstep,
  step,
  texture,
  uniform,
  vec3,
  vec4,
} from 'three/tsl'
import { meshIslands } from './meshIslands'

const library = new StandardNodeLibrary()
const underwearMasks = new WeakMap<THREE.BufferGeometry, Float32Array>()

function underwearMask(geometry: THREE.BufferGeometry) {
  let values = underwearMasks.get(geometry)
  if (values) return values
  const { find, islands } = meshIslands(geometry, 1e6)
  const underwear = new Set(
    [...islands]
      .filter(([, { bounds }]) => bounds.min.y > 0.85 && bounds.max.y < 1.55)
      .map(([id]) => id)
  )
  values = new Float32Array(geometry.attributes.position.count)
  for (let i = 0; i < values.length; i++)
    values[i] = underwear.has(find(i)) ? 1 : 0
  underwearMasks.set(geometry, values)
  return values
}

export function femaleUnderwearColor(mesh: THREE.SkinnedMesh) {
  const originalGeometry = mesh.geometry
  const geometry = (mesh.geometry = originalGeometry.clone())
  const originalMaterial = mesh.material
  const originals = Array.isArray(originalMaterial)
    ? originalMaterial
    : [originalMaterial]
  geometry.setAttribute(
    'rogueUnderwear',
    new THREE.BufferAttribute(underwearMask(originalGeometry), 1)
  )
  const enabled = uniform(0)
  const skinTint = uniform(0)
  const skinColor = uniform(
    new THREE.Color().setRGB(
      194 / 255,
      151 / 255,
      125 / 255,
      THREE.SRGBColorSpace
    )
  )
  const materials: MeshStandardNodeMaterial[] = []
  const tinted = originals.map((source) => {
    if (!(source instanceof THREE.MeshStandardMaterial)) return source
    const material = library.fromMaterial(
      source.clone()
    ) as MeshStandardNodeMaterial
    const base = source.map ? texture(source.map) : vec4(source.color, 1)
    const palest = min(base.r, min(base.g, base.b))
    const brightest = max(base.r, max(base.g, base.b))
    const print = enabled
      .mul(step(0.88, positionGeometry.y))
      .mul(step(positionGeometry.y, 1.13))
      .mul(smoothstep(0.22, 0.3, palest))
      .mul(smoothstep(0.4, 0.55, palest.div(max(brightest, 0.001))))
    const dark = max(print, enabled.mul(attribute('rogueUnderwear', 'float')))
    const finish = max(
      dark,
      enabled
        .mul(smoothstep(1.06, 1.075, positionGeometry.y))
        .mul(smoothstep(1.125, 1.14, positionGeometry.y).oneMinus())
    )
    const darken = mix(vec3(1), vec3(0.008, 0.009, 0.012), dark)
    const skinPrint = skinTint
      .mul(step(0.88, positionGeometry.y))
      .mul(step(positionGeometry.y, 1.13))
      .mul(smoothstep(0.24, 0.42, palest))
      .mul(smoothstep(0.52, 0.72, palest.div(max(brightest, 0.001))))
    material.colorNode = vec4(
      mix(materialColor.rgb.mul(darken), skinColor, skinPrint),
      base.a
    )
    material.roughnessNode = mix(
      mix(materialRoughness, max(materialRoughness, 0.85), finish),
      0.65,
      skinPrint
    )
    material.metalnessNode = materialMetalness.mul(
      max(finish, skinPrint).oneMinus()
    )
    materials.push(material)
    return material
  })
  mesh.material = Array.isArray(originalMaterial) ? tinted : tinted[0]
  return Object.assign(
    (black: boolean, skin = false) => {
      enabled.value = black ? 1 : 0
      skinTint.value = skin && !black ? 1 : 0
    },
    {
      dispose() {
        for (const material of materials) material.dispose()
        geometry.dispose()
        mesh.geometry = originalGeometry
        mesh.material = originalMaterial
      },
    }
  )
}
