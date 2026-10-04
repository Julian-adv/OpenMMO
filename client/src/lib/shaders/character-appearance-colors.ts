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
import { region, skinnedParts } from '../utils/modularCharacter'

const library = new StandardNodeLibrary()
const colors = new WeakMap<Object3D, AppearanceColors>()

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
      const hair = id === 'hair_crop' || id === 'hair_wavy_bone'
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
            const point = vec2(
              abs(positionGeometry.x).sub(0.034),
              positionGeometry.y.sub(1.7825)
            )
            const radius = point.div(vec2(0.005, 0.0045)).length()
            const mask = smoothstep(0.85, 1, radius)
              .oneMinus()
              .mul(smoothstep(0.26, 0.48, radius))
              .mul(step(0.07, positionGeometry.z))
            const iris = this.eyes.mul(
              luminance(base.rgb).mul(6).clamp(0.15, 1.2)
            )
            material.colorNode = vec4(
              mix(skin.rgb, iris, mask.mul(0.9)),
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
