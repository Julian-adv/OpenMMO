import {
  Color,
  Material,
  MeshStandardMaterial,
  type Object3D,
  type SkinnedMesh,
} from 'three'
import { StandardNodeLibrary, type Node, type NodeMaterial } from 'three/webgpu'
import {
  abs,
  convert,
  luminance,
  materialColor,
  mix,
  positionGeometry,
  smoothstep,
  step,
  texture,
  uniform,
  vec2,
  vec4,
} from 'three/tsl'
import type { CharacterAppearance } from '../network/networkTypes'
import {
  DEFAULT_EYE_COLOR,
  DEFAULT_HAIR_COLOR,
  FEMALE_HAIR_COLORS,
} from '../utils/appearanceColors'
import { FEMALE_HAIR_PARTS } from '../utils/femaleCharacter'
import {
  SELECTABLE_HAIR_PARTS,
  region,
  skinnedParts,
} from '../utils/modularCharacter'

const library = new StandardNodeLibrary()
const colors = new WeakMap<Object3D, AppearanceColors>()
type Eye = {
  x: readonly [number, number]
  y: number
  radius: readonly [number, number]
  front?: number
  // Texture redness and luminance ranges faded out of the iris mask.
  redCut?: readonly [number, number]
  detailCut?: readonly [number, number]
}
const DEFAULT_EYE: Eye = {
  x: [0.034, 0.034],
  y: 1.7825,
  radius: [0.005, 0.0045],
}
// The ranger fit leaves its eyes off-center; its wider mask must skip baked skin and sclera.
const RANGER_EYE: Eye = {
  x: [0.0354, 0.0358],
  y: 1.7918,
  radius: [0.0072, 0.0065],
  redCut: [0.045, 0.08],
  detailCut: [0.18, 0.28],
}
// The female iris is brown like her skin, so only luminance separates it from skin and sclera.
const FEMALE_EYE: Eye = {
  x: [0.0316, 0.0333],
  y: 1.6852,
  radius: [0.0064, 0.0064],
  front: 0.05,
  detailCut: [0.12, 0.2],
}
// Mean linear luminance of each female hair texture, so the chosen color is the mean albedo.
const linearLuminance = (hex: string) => {
  const { r, g, b } = new Color(hex)
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}
const FEMALE_HAIR_LUMINANCE: Record<string, number> = Object.fromEntries(
  Object.entries(FEMALE_HAIR_PARTS).map(([hair, part]) => [
    part,
    linearLuminance(
      FEMALE_HAIR_COLORS[hair as keyof typeof FEMALE_HAIR_COLORS]
    ),
  ])
)
// The barbarian helmet hair is a cut of the bob texture.
FEMALE_HAIR_LUMINANCE.barbarian_hair = FEMALE_HAIR_LUMINANCE.hair

class AppearanceColors {
  readonly hair = uniform(new Color(DEFAULT_HAIR_COLOR))
  readonly eyes = uniform(new Color(DEFAULT_EYE_COLOR))
  private readonly materials = new Map<Material, NodeMaterial>()
  private readonly originals = new WeakMap<Material, Material>()

  apply(meshes: SkinnedMesh[], appearance?: CharacterAppearance): void {
    this.hair.value.set(appearance?.hair_color ?? DEFAULT_HAIR_COLOR)
    this.eyes.value.set(appearance?.eye_color ?? DEFAULT_EYE_COLOR)
    for (const mesh of meshes) {
      if (!mesh.visible) continue
      const id = mesh.userData.part_id
      const femaleHair = FEMALE_HAIR_LUMINANCE[mesh.userData.female_part]
      const hair = SELECTABLE_HAIR_PARTS.has(id) || femaleHair !== undefined
      const female = !!mesh.userData.female_body
      if (!hair && !female && region(mesh) !== 'head') continue
      const tint = (current: Material) => {
        const source = this.originals.get(current) ?? current
        if (source.userData.appearance_color_fixed) return current
        const node = (source as NodeMaterial).isNodeMaterial
        if (!node && !(source instanceof MeshStandardMaterial)) return current
        let material = this.materials.get(source)
        if (!material) {
          // The female body already carries node colors (underwear), so tint on top of them.
          material = node
            ? (source.clone() as NodeMaterial)
            : library.fromMaterial(source.clone())
          const { map, color } = source as MeshStandardMaterial
          const base = map ? texture(map) : vec4(color, 1)
          if (hair) {
            const detail = luminance(base.rgb)
            const brightness =
              femaleHair !== undefined
                ? detail.div(femaleHair).clamp(0, 2.5)
                : id === 'hair_wavy_bone'
                  ? detail.div(0.12).clamp(0, 2)
                  : detail.mul(3).clamp(0, 1.5)
            material.colorNode = vec4(this.hair.mul(brightness), base.a)
          } else {
            const skin = ((source as NodeMaterial).colorNode ??
              convert(materialColor, 'vec4')) as Node<'vec4'>
            const faceEye =
              appearance?.face === 'ranger' ? RANGER_EYE : DEFAULT_EYE
            const eye = female ? FEMALE_EYE : faceEye
            const point = vec2(
              abs(positionGeometry.x).sub(
                mix(eye.x[0], eye.x[1], step(0, positionGeometry.x))
              ),
              positionGeometry.y.sub(eye.y)
            )
            const radius = point.div(vec2(...eye.radius)).length()
            const pupil = point.div(vec2(...DEFAULT_EYE.radius)).length()
            const detail = luminance(base.rgb)
            const mask = smoothstep(0.85, 1, radius)
              .oneMinus()
              .mul(smoothstep(0.26, 0.48, pupil))
              .mul(step(eye.front ?? 0.07, positionGeometry.z))
            const red = eye.redCut
              ? smoothstep(...eye.redCut, base.r.sub(base.g)).oneMinus()
              : 1
            const bright = eye.detailCut
              ? smoothstep(...eye.detailCut, detail).oneMinus()
              : 1
            const irisMask = mask.mul(red).mul(bright)
            const iris = this.eyes.mul(detail.mul(6).clamp(0.15, 1.2))
            material.colorNode = vec4(
              mix(skin.rgb, iris, irisMask.mul(0.9)),
              skin.a
            )
          }
          this.materials.set(source, material)
          this.originals.set(material, source)
        }
        return material
      }
      mesh.material = Array.isArray(mesh.material)
        ? mesh.material.map(tint)
        : tint(mesh.material)
    }
  }

  dispose(root: Object3D): void {
    for (const mesh of skinnedParts(root)) {
      const restore = (material: Material) =>
        this.originals.get(material) ?? material
      mesh.material = Array.isArray(mesh.material)
        ? mesh.material.map(restore)
        : restore(mesh.material)
    }
    for (const material of this.materials.values()) material.dispose()
    this.materials.clear()
  }
}

export function applyAppearanceColors(
  root: Object3D,
  meshes: SkinnedMesh[],
  appearance?: CharacterAppearance
): void {
  let instance = colors.get(root)
  if (!instance) {
    instance = new AppearanceColors()
    colors.set(root, instance)
  }
  instance.apply(meshes, appearance)
}

export function disposeAppearanceColors(root: Object3D): void {
  colors.get(root)?.dispose(root)
  colors.delete(root)
}
