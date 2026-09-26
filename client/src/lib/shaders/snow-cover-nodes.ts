import type * as THREE from 'three'
import { MeshStandardNodeMaterial, type Node } from 'three/webgpu'
import {
  Fn,
  If,
  uniform,
  convert,
  vec2,
  vec3,
  float,
  mix,
  smoothstep,
  positionWorld,
  normalWorld,
  normalWorldGeometry,
  materialColor,
  materialRoughness,
  vec4,
  dot,
  max,
  fwidth,
  mx_noise_float,
} from 'three/tsl'

/** 0..1 snow lying around the player, read by every snowy material; one
 *  value covers the view since a cell's edge is hundreds of metres wide. */
export const snowCover = uniform(0)

interface SnowSurface {
  color: Node<'vec3'>
  normal?: Node<'vec3'>
  roughness?: Node<'float'>
  ao?: Node<'float'>
}

/** Perlin in [0, 1], turned per octave so no lattice line runs along a world
 *  axis. Procedural because value-noise.jpg does not tile. */
function perlin(
  xz: Node<'vec2'>,
  scale: number,
  angle: number,
  offset: number
) {
  const c = Math.cos(angle) * scale
  const s = Math.sin(angle) * scale
  const p = vec2(
    xz.x.mul(c).sub(xz.y.mul(s)),
    xz.x.mul(s).add(xz.y.mul(c))
  ).add(offset)
  return mx_noise_float(p).mul(0.5).add(0.5)
}

/** Positive where snow lies: flat ground and noise hollows whiten first,
 *  slopes need a deeper cover and cliffs stay bare. */
function snowEdge(
  cover: Node<'float'>,
  xz: Node<'vec2'>,
  upness: Node<'float'>,
  crevice: Node<'float'> = float(0)
): Node<'float'> {
  const n = perlin(xz, 0.06, 0.61, 0)
    .mul(0.35)
    .add(perlin(xz, 0.3, 1.97, 17.3).mul(0.4))
    .add(perlin(xz, 2.6, 2.9, 53.1).mul(0.25))
  const coverage = cover.mul(smoothstep(0.5, 0.85, upness))
  return coverage
    .mul(1.15)
    .sub(smoothstep(0.3, 0.7, n).mul(0.9).add(0.05))
    .add(crevice.mul(0.06))
}

/** 0..1 snow mask for the fragment being shaded. Just outside the patches,
 *  fine specks read as a dusting rather than a hard border. `crevice` (0..1)
 *  fills first, e.g. the grout of paving. */
export function snowMask(
  upness: Node<'float'>,
  gate: Node<'float'> = float(1),
  crevice?: Node<'float'>
): Node<'float'> {
  return Fn(() => {
    const mask = float(0).toVar()
    const cover = snowCover.mul(gate)
    If(cover.greaterThan(0.001), () => {
      const p = positionWorld.xz
      const edge = snowEdge(cover, p, upness, crevice)
      const width = max(fwidth(edge).mul(0.75), 0.035)
      const patch = smoothstep(width.negate(), width, edge)
      const speck = perlin(p, 7.0, 0.37, 91.7)
      // Crevices hold the dusting; letting them move the patch edge made
      // it follow the paving stones in steps.
      const dust = smoothstep(-0.3, 0, edge)
        .mul(max(smoothstep(0.52, 0.62, speck), crevice ?? float(0)))
        .mul(0.8)
      mask.assign(max(patch, dust).mul(smoothstep(0.02, 0.15, positionWorld.y)))
    })
    return mask
  })().toVar()
}

/** 0..1 snow depth at a ground point, for vertex stages such as grass roots. */
export function snowBurial(xz: Node<'vec2'>): Node<'float'> {
  return Fn(() => {
    const burial = float(0).toVar()
    If(snowCover.greaterThan(0.001), () => {
      burial.assign(smoothstep(-0.08, 0.08, snowEdge(snowCover, xz, float(1))))
    })
    return burial
  })()
}

/** Blends a material toward snow where `snowMask` is set; around the patches
 *  a thin frost pales the ground so thin cover reads as dusting. */
export function applySnowCover(
  material: MeshStandardNodeMaterial,
  snow: SnowSurface,
  gate?: Node<'float'>
): Node<'float'> {
  const bedAo = material.aoNode as Node<'float'> | null
  const crevice = bedAo ? smoothstep(0.95, 0.4, bedAo) : undefined
  const mask = snowMask(normalWorldGeometry.y, gate, crevice)
  const frost = snowCover
    .mul(gate ?? float(1))
    .mul(smoothstep(0.5, 0.85, normalWorldGeometry.y))
    .mul(0.35)
  // The snow texture samples use explicit gradients, so they can sit behind
  // the mask branch and cost nothing on bare ground.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const onSnow = (bed: any, snowy: any) =>
    Fn(() => {
      const value = bed.toVar()
      If(mask.greaterThan(0.001), () => {
        value.assign(mix(bed, snowy, mask))
      })
      return value
    })()
  const bedColor = material.colorNode
  if (bedColor) {
    material.colorNode = Fn(() => {
      const color = (convert(bedColor, 'vec4') as Node<'vec4'>).toVar()
      const luma = dot(color.rgb, vec3(0.299, 0.587, 0.114))
      const frosted = mix(color.rgb, vec3(luma.mul(0.6).add(0.35)), frost)
      color.rgb.assign(onSnow(frosted, snow.color))
      return color
    })()
  }
  const bedNormal = material.normalNode as Node<'vec3'> | null
  if (snow.normal && bedNormal) {
    material.normalNode = onSnow(bedNormal, snow.normal).normalize()
  }
  const bedRoughness =
    (material.roughnessNode as Node<'float'> | null) ??
    float(material.roughness)
  material.roughnessNode = onSnow(bedRoughness, snow.roughness ?? float(0.85))
  const bedMetalness = material.metalnessNode as Node<'float'> | null
  if (bedMetalness) material.metalnessNode = mix(bedMetalness, float(0), mask)
  if (bedAo) material.aoNode = onSnow(bedAo, snow.ao ?? float(1))
  return mask
}

const OBJECT_SNOW = vec3(0.86, 0.88, 0.92)

/** Node copy of a GLB material whose upward faces gather the lying snow;
 *  vertex normals keep rounded canopies white on top only. */
export function snowyStandardMaterial(
  source: THREE.MeshStandardMaterial
): MeshStandardNodeMaterial {
  // copy() skips the node slots a plain material lacks, so recopying after
  // the source changes keeps the snow.
  const material = new MeshStandardNodeMaterial().copy(
    source
  ) as MeshStandardNodeMaterial
  const mask = snowMask(normalWorld.y)
  // The map's alpha rides in materialColor, so cut-out leaves keep it.
  const base = convert(materialColor, 'vec4') as Node<'vec4'>
  material.colorNode = vec4(mix(base.rgb, OBJECT_SNOW, mask), base.a)
  material.roughnessNode = mix(materialRoughness, float(0.85), mask)
  return material
}
