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
} from '../utils/appearanceColors'
import {
  SELECTABLE_HAIR_PARTS,
  region,
  skinnedParts,
} from '../utils/modularCharacter'

const library = new StandardNodeLibrary()
const colors = new WeakMap<Object3D, AppearanceColors>()
const DEFAULT_EYE = {
  x: [0.034, 0.034],
  y: 1.7825,
  radius: [0.005, 0.0045],
  excludeSkin: false,
} as const
// The ranger fit leaves its eyes off-center; its wider mask must skip baked skin and sclera.
const RANGER_EYE = {
  x: [0.0354, 0.0358],
  y: 1.7918,
  radius: [0.0072, 0.0065],
  excludeSkin: true,
} as const

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
      const hair = SELECTABLE_HAIR_PARTS.has(id)
      if (!hair && region(mesh) !== 'head') continue
      const tint = (current: Material) => {
        const source = this.originals.get(current) ?? current
        if (source.userData.appearance_color_fixed) return current
        if (!(source instanceof MeshStandardMaterial)) return current
        let material = this.materials.get(source)
        if (!material) {
          material = library.fromMaterial(source.clone())
          const base = source.map ? texture(source.map) : vec4(source.color, 1)
          if (hair) {
            const detail = luminance(base.rgb)
            const brightness =
              id === 'hair_wavy_bone'
                ? detail.div(0.12).clamp(0, 2)
                : detail.mul(3).clamp(0, 1.5)
            material.colorNode = vec4(this.hair.mul(brightness), base.a)
          } else {
            const skin = convert(materialColor, 'vec4') as Node<'vec4'>
            const eye = appearance?.face === 'ranger' ? RANGER_EYE : DEFAULT_EYE
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
              .mul(step(0.07, positionGeometry.z))
            const irisMask = eye.excludeSkin
              ? mask
                  .mul(smoothstep(0.045, 0.08, base.r.sub(base.g)).oneMinus())
                  .mul(smoothstep(0.18, 0.28, detail).oneMinus())
              : mask
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
