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
  float,
  luminance,
  max,
  materialColor,
  mix,
  positionGeometry,
  smoothstep,
  step,
  texture,
  uniform,
  uv,
  vec2,
  vec4,
} from 'three/tsl'
import { seeThroughSource } from '../effects/see-through'
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
const colors = new WeakMap<Object3D, Tints>()
type Eye = {
  x: readonly [number, number]
  y: number
  radius: readonly [number, number]
  lowerRadius?: number
  edge?: readonly [number, number]
  upperLid?: readonly (readonly [number, number])[]
  front?: number
  // Color limits exclude skin and sclera.
  redCut?: readonly [number, number]
  detailCut?: readonly [number, number]
  blueCut?: readonly [number, number]
  // The mesh marks its eyeballs with uv1.x = 1, keeping the tint off the eyelids in front.
  eyeballUv?: boolean
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
// Iris centres and radius from the ranger face fit (fitting.json `irises`).
const RANGER_FEMALE_EYE: Eye = {
  x: [0.0319, 0.031],
  y: 1.6842,
  radius: [0.0068, 0.0068],
  front: 0.03,
  eyeballUv: true,
}
// Iris centres and radius from the chic face fit (fitting.json `irises`); its narrow lids hide the rest.
const CHIC_FEMALE_EYE: Eye = {
  x: [0.0295, 0.0297],
  y: 1.6848,
  radius: [0.0054, 0.0054],
  front: 0.03,
  eyeballUv: true,
}
// Iris centres and radius from the elegant face fit (fitting.json `irises`).
const ELEGANT_FEMALE_EYE: Eye = {
  x: [0.0335, 0.0331],
  y: 1.6865,
  radius: [0.0075, 0.0074],
  front: 0.03,
  eyeballUv: true,
}
const FEMALE_FACE_EYES: Record<string, Eye> = {
  face_ranger: RANGER_FEMALE_EYE,
  face_chic: CHIC_FEMALE_EYE,
  face_elegant: ELEGANT_FEMALE_EYE,
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

export type Tints = {
  hair: Color
  eyes: Color
  hairTint: number
  eyeTint: number
  customEyeTint: number
}

// Read per mesh, so every character shares one tinted material (and shader)
// per source material and look.
const tintsOf = (object: Object3D | null | undefined): Tints | undefined =>
  object?.userData.appearanceTints
const hairColor = uniform(new Color(DEFAULT_HAIR_COLOR)).onObjectUpdate(
  ({ object }, self) => self.value.copy(tintsOf(object)?.hair ?? self.value)
)
const eyeColor = uniform(new Color(DEFAULT_EYE_COLOR)).onObjectUpdate(
  ({ object }, self) => self.value.copy(tintsOf(object)?.eyes ?? self.value)
)
const hairTint = uniform(1).onObjectUpdate(
  ({ object }) => tintsOf(object)?.hairTint ?? 1
)
const eyeTint = uniform(1).onObjectUpdate(
  ({ object }) => tintsOf(object)?.eyeTint ?? 1
)
const customEyeTint = uniform(0).onObjectUpdate(
  ({ object }) => tintsOf(object)?.customEyeTint ?? 0
)
const tintedMaterials = new WeakMap<Material, Map<string, NodeMaterial>>()
const originals = new WeakMap<Material, Material>()

function applyTints(
  tints: Tints,
  meshes: SkinnedMesh[],
  appearance?: CharacterAppearance
): void {
  tints.hair.set(appearance?.hair_color ?? DEFAULT_HAIR_COLOR)
  tints.hairTint = appearance?.hair_color_original ? 0 : 1
  tints.eyeTint = appearance?.eye_color_original ? 0 : 1
  tints.eyes.set(appearance?.eye_color ?? DEFAULT_EYE_COLOR)
  tints.customEyeTint =
    (appearance?.eye_color ?? DEFAULT_EYE_COLOR) === DEFAULT_EYE_COLOR ? 0 : 1
  for (const mesh of meshes) {
    if (!mesh.visible) continue
    const id = mesh.userData.part_id
    const femaleHair = FEMALE_HAIR_LUMINANCE[mesh.userData.female_part]
    const hair = SELECTABLE_HAIR_PARTS.has(id) || femaleHair !== undefined
    const customFace = !!mesh.userData.female_face
    const female = !!mesh.userData.female_body || customFace
    if (!hair && !female && region(mesh) !== 'head') continue
    mesh.userData.appearanceTints = tints
    const look = hair
      ? `hair:${femaleHair ?? id}`
      : customFace
        ? `face:${mesh.userData.female_part}`
        : female
          ? 'female'
          : `head:${appearance?.face === 'ranger' ? 'ranger' : 'default'}`
    const tint = (worn: Material) => {
      const current = seeThroughSource(worn) ?? worn
      const source = originals.get(current) ?? current
      if (source.userData.appearance_color_fixed) return current
      const node = (source as NodeMaterial).isNodeMaterial
      if (!node && !(source instanceof MeshStandardMaterial)) return current
      let looks = tintedMaterials.get(source)
      if (!looks) tintedMaterials.set(source, (looks = new Map()))
      let material = looks.get(look)
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
          const original = ((source as NodeMaterial).colorNode ??
            convert(materialColor, 'vec4')) as Node<'vec4'>
          material.colorNode = vec4(
            mix(original.rgb, hairColor.mul(brightness), hairTint),
            original.a
          )
        } else {
          const skin = ((source as NodeMaterial).colorNode ??
            convert(materialColor, 'vec4')) as Node<'vec4'>
          const faceEye =
            appearance?.face === 'ranger' ? RANGER_EYE : DEFAULT_EYE
          const eye = customFace
            ? (FEMALE_FACE_EYES[mesh.userData.female_part] ?? RANGER_FEMALE_EYE)
            : female
              ? FEMALE_EYE
              : faceEye
          const point = vec2(
            abs(positionGeometry.x).sub(
              mix(eye.x[0], eye.x[1], step(0, positionGeometry.x))
            ),
            positionGeometry.y.sub(eye.y)
          )
          const verticalRadius = eye.lowerRadius
            ? mix(eye.lowerRadius, eye.radius[1], step(0, point.y))
            : eye.radius[1]
          const radius = point.div(vec2(eye.radius[0], verticalRadius)).length()
          const pupil = point.div(vec2(...DEFAULT_EYE.radius)).length()
          const detail = luminance(base.rgb)
          const mask = smoothstep(...(eye.edge ?? [0.85, 1]), radius)
            .oneMinus()
            .mul(smoothstep(0.26, 0.48, pupil))
            .mul(step(eye.front ?? 0.07, positionGeometry.z))
          const red = eye.redCut
            ? smoothstep(...eye.redCut, base.r.sub(base.g)).oneMinus()
            : 1
          const bright = eye.detailCut
            ? smoothstep(...eye.detailCut, detail).oneMinus()
            : 1
          const pigment = eye.blueCut
            ? smoothstep(
                ...eye.blueCut,
                base.b.div(max(base.g, 0.001))
              ).oneMinus()
            : 1
          let lidMask: Node<'float'> | number = 1
          if (eye.upperLid) {
            const knots = eye.upperLid
            const x = abs(positionGeometry.x.add((eye.x[0] - eye.x[1]) / 2))
            const upper = knots
              .slice(1)
              .reduce<Node<'float'>>((edge, [stop, y], i) => {
                const [start, previous] = knots[i]
                return edge.add(
                  x
                    .sub(start)
                    .div(stop - start)
                    .clamp(0, 1)
                    .mul(y - previous)
                )
              }, float(knots[0][1]))
            lidMask = smoothstep(
              upper.sub(0.0001),
              upper.add(0.00025),
              positionGeometry.y
            ).oneMinus()
          }
          const irisMask = mask
            .mul(red)
            .mul(bright)
            .mul(pigment)
            .mul(lidMask)
            .mul(eye.eyeballUv ? uv(1).x : 1)
          const iris = eyeColor.mul(detail.mul(6).clamp(0.15, 1.2))
          material.colorNode = vec4(
            mix(
              skin.rgb,
              iris,
              irisMask.mul(customFace ? customEyeTint : 0.9).mul(eyeTint)
            ),
            skin.a
          )
        }
        looks.set(look, material)
        originals.set(material, source)
      }
      return material
    }
    mesh.material = Array.isArray(mesh.material)
      ? mesh.material.map(tint)
      : tint(mesh.material)
  }
}

export function applyAppearanceColors(
  root: Object3D,
  meshes: SkinnedMesh[],
  appearance?: CharacterAppearance
): void {
  let tints = colors.get(root)
  if (!tints) {
    tints = {
      hair: new Color(DEFAULT_HAIR_COLOR),
      eyes: new Color(DEFAULT_EYE_COLOR),
      hairTint: 1,
      eyeTint: 1,
      customEyeTint: 0,
    }
    colors.set(root, tints)
  }
  applyTints(tints, meshes, appearance)
}

export function disposeAppearanceColors(root: Object3D): void {
  for (const mesh of skinnedParts(root)) {
    const restore = (material: Material) => originals.get(material) ?? material
    mesh.material = Array.isArray(mesh.material)
      ? mesh.material.map(restore)
      : restore(mesh.material)
    delete mesh.userData.appearanceTints
  }
  colors.delete(root)
}
