import * as THREE from 'three'
import {
  StandardNodeLibrary,
  type MeshStandardNodeMaterial,
} from 'three/webgpu'
import {
  attribute,
  materialColor,
  materialMetalness,
  materialNormal,
  materialRoughness,
  max,
  min,
  mix,
  normalView,
  positionGeometry,
  smoothstep,
  step,
  texture,
  uniform,
  vec2,
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

type Tints = { enabled: number; skin: number; plate: number }

const tintsOf = (object: THREE.Object3D): Tints | undefined =>
  object.userData.femaleUnderwear
// Read per mesh, so every female body shares one material and one shader.
const enabled = uniform(0).onObjectUpdate(
  ({ object }) => tintsOf(object!)?.enabled ?? 0
)
const skinTint = uniform(0).onObjectUpdate(
  ({ object }) => tintsOf(object!)?.skin ?? 0
)
const plateTint = uniform(0).onObjectUpdate(
  ({ object }) => tintsOf(object!)?.plate ?? 0
)
const skinColor = uniform(
  // The body's skin after tools/match-female-body-skin.py (was 194, 151, 125).
  new THREE.Color().setRGB(
    226 / 255,
    157 / 255,
    117 / 255,
    THREE.SRGBColorSpace
  )
)
const tintedMaterials = new WeakMap<THREE.Material, MeshStandardNodeMaterial>()

function tinted(source: THREE.MeshStandardMaterial) {
  const cached = tintedMaterials.get(source)
  if (cached) return cached
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
  const bareHip = plateTint
    .mul(smoothstep(0.865, 0.898, positionGeometry.y))
    .mul(smoothstep(1.108, 1.145, positionGeometry.y).oneMinus())
    .mul(smoothstep(0.15, 0.185, positionGeometry.x.abs()).oneMinus())
  const thighColor = source.map
    ? texture(source.map, vec2(0.497, 0.867)).rgb
    : skinColor
  material.normalNode = mix(materialNormal, normalView, bareHip).normalize()
  material.colorNode = vec4(
    mix(
      mix(materialColor.rgb.mul(darken), skinColor, skinPrint),
      thighColor,
      bareHip
    ),
    base.a
  )
  material.roughnessNode = mix(
    mix(materialRoughness, max(materialRoughness, 0.85), finish),
    0.65,
    max(skinPrint, bareHip)
  )
  material.metalnessNode = materialMetalness.mul(
    max(finish, max(skinPrint, bareHip)).oneMinus()
  )
  tintedMaterials.set(source, material)
  return material
}

export function femaleUnderwearColor(mesh: THREE.SkinnedMesh) {
  const originalGeometry = mesh.geometry
  const geometry = (mesh.geometry = originalGeometry.clone())
  const originalMaterial = mesh.material
  geometry.setAttribute(
    'rogueUnderwear',
    new THREE.BufferAttribute(underwearMask(originalGeometry), 1)
  )
  const swap = (source: THREE.Material) =>
    source instanceof THREE.MeshStandardMaterial ? tinted(source) : source
  mesh.material = Array.isArray(originalMaterial)
    ? originalMaterial.map(swap)
    : swap(originalMaterial)
  const tints: Tints = (mesh.userData.femaleUnderwear = {
    enabled: 0,
    skin: 0,
    plate: 0,
  })
  return Object.assign(
    (black: boolean, skin = false, plate = false) => {
      tints.plate = plate ? 1 : 0
      tints.enabled = black ? 1 : 0
      tints.skin = skin && !black ? 1 : 0
    },
    {
      dispose() {
        geometry.dispose()
        delete mesh.userData.femaleUnderwear
        mesh.geometry = originalGeometry
        mesh.material = originalMaterial
      },
    }
  )
}
