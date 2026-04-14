// makeSplatStandardMaterial.ts — TSL/WebGPU, V2 palette-based splatmap
// Per-cell encoding (see doc/SPLATMAP_V2.md):
//   byte 0: (primaryIdx << 4) | secondaryIdx  (each 0..15)
//   byte 1: reserved
//   byte 2: blend (0 = 100% primary, 255 = 100% secondary)
//   byte 3: vegMeta (grass density / subtype, read by grass system, not shader)
import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import {
  Fn,
  uniform,
  uniformArray,
  texture,
  uv,
  vec2,
  vec3,
  vec4,
  float,
  int,
  step,
  smoothstep,
  mix,
  min,
  max,
  varying,
  positionLocal,
  modelWorldMatrix,
  fwidth,
  fract,
  abs,
  distance,
  dFdx,
  dFdy,
  TBNViewMatrix,
} from 'three/tsl'
import type TextureNode from 'three/src/nodes/accessors/TextureNode.js'
import type { ShaderNodeObject } from 'three/src/nodes/tsl/TSLCore.js'
import {
  ATLAS_BORDER,
  ATLAS_GRID,
  ATLAS_SLOT_SIZE,
  type SplatAtlasSet,
} from '../utils/splatLayerLoader'
import { MAX_PALETTE } from '../terrain/splat-encoding'
import { TILE_DIM } from '../terrain/terrain-constants'

export type SplatLayer = {
  map: THREE.Texture // Albedo (sRGB)
  normalMap?: THREE.Texture // Normal (Linear)
  orm?: THREE.Texture // ORM: R=AO, G=Roughness, B=Metallic (Linear)
  tile: number
}

export type SplatParams = {
  atlas: SplatAtlasSet
  /** Tile scales for each palette slot. Length 1..MAX_PALETTE; padded to MAX_PALETTE internally. */
  tileScales: number[]
  splatMap: THREE.Texture
  splatScale?: number
  sharedBrushUniforms?: SplatBrushUniforms
  /** Include grid/brush editor overlay in the shader. Default false. */
  includeEditorOverlay?: boolean
}

export interface SplatBrushUniforms {
  brushCenter: ReturnType<typeof uniform<THREE.Vector2>>
  brushRadius: ReturnType<typeof uniform<number>>
  brushActive: ReturnType<typeof uniform<number>>
  brushRaise: ReturnType<typeof uniform<number>>
  brushToolMode: ReturnType<typeof uniform<number>>
  gridVisible: ReturnType<typeof uniform<number>>
}

export function createSplatBrushUniforms(): SplatBrushUniforms {
  return {
    brushCenter: uniform(new THREE.Vector2(0, 0)),
    brushRadius: uniform(3.0),
    brushActive: uniform(0.0),
    brushRaise: uniform(1.0),
    brushToolMode: uniform(0.0),
    gridVisible: uniform(0.0),
  }
}

/** Pad a tileScales array to length MAX_PALETTE with 1.0. Returns a new array. */
export function padTileScales(tileScales: number[]): number[] {
  const out = new Array<number>(MAX_PALETTE).fill(1)
  const n = Math.min(tileScales.length, MAX_PALETTE)
  for (let i = 0; i < n; i++) out[i] = tileScales[i]
  return out
}

// Atlas slot geometry in normalized UV space.
const SLOT_PX = ATLAS_SLOT_SIZE + 2 * ATLAS_BORDER
const ATLAS_PX = SLOT_PX * ATLAS_GRID
const SUBTEX_NORM = ATLAS_SLOT_SIZE / ATLAS_PX
const BORDER_NORM = ATLAS_BORDER / ATLAS_PX
const GRID_INV = 1.0 / ATLAS_GRID

export function makeSplatStandardMaterial({
  atlas,
  tileScales,
  splatMap,
  splatScale = 1,
  sharedBrushUniforms,
  includeEditorOverlay = false,
}: SplatParams) {
  // Splat bytes are integer indices — must NOT be bilinearly interpolated.
  splatMap.wrapS = splatMap.wrapT = THREE.RepeatWrapping
  splatMap.minFilter = THREE.NearestFilter
  splatMap.magFilter = THREE.NearestFilter
  splatMap.generateMipmaps = false
  splatMap.anisotropy = 1
  splatMap.needsUpdate = true

  const uTileScales = uniformArray(padTileScales(tileScales), 'float')
  const uSplatScale = uniform(splatScale)

  const brush = includeEditorOverlay
    ? {
        center:
          sharedBrushUniforms?.brushCenter ?? uniform(new THREE.Vector2(0, 0)),
        radius: sharedBrushUniforms?.brushRadius ?? uniform(3.0),
        active: sharedBrushUniforms?.brushActive ?? uniform(0.0),
        raise: sharedBrushUniforms?.brushRaise ?? uniform(1.0),
        toolMode: sharedBrushUniforms?.brushToolMode ?? uniform(0.0),
        gridVisible: sharedBrushUniforms?.gridVisible ?? uniform(0.0),
      }
    : null

  const splatTex = texture(splatMap)
  const diffAtlasTex = texture(atlas.diffuseAtlas)
  const normAtlasTex = atlas.normalAtlas ? texture(atlas.normalAtlas) : null
  const ormAtlasTex = atlas.ormAtlas ? texture(atlas.ormAtlas) : null

  const vUvSplat = varying(vec2(0), 'v_uvSplat')
  const vWorldXZ = varying(vec2(0), 'v_worldXZ')

  const vertexNode = Fn(() => {
    const localUv = uv()
    vUvSplat.assign(localUv.mul(uSplatScale))
    const worldPos4 = modelWorldMatrix.mul(vec4(positionLocal, 1.0))
    vWorldXZ.assign(worldPos4.xz)
    return positionLocal
  })()

  // ─── Decode splat cells + weight-space bilinear blend ────────────
  //
  // P/S indices come from the nearest cell (whichever of the 4 taps the pixel
  // sits in). The `blend` factor is interpolated across the 4 cells in weight
  // space: each neighbor contributes its fraction of texture P and texture S.
  // This smooths transitions across cells that share at least one slot
  // (e.g. (grass,sand) ↔ (grass,laterite)). When neighbors share no slot,
  // pW+sW collapses to ~0 and we fall back to the nearest cell's blend.
  const SPLAT_TEXEL = 1.0 / TILE_DIM

  const cellPos = vUvSplat.mul(float(TILE_DIM)).sub(0.5)
  const baseUv = cellPos.floor().add(0.5).mul(SPLAT_TEXEL)
  const fracUv = fract(cellPos)
  const s00 = splatTex.sample(baseUv).toVar()
  const s10 = splatTex.sample(baseUv.add(vec2(SPLAT_TEXEL, 0))).toVar()
  const s01 = splatTex.sample(baseUv.add(vec2(0, SPLAT_TEXEL))).toVar()
  const s11 = splatTex
    .sample(baseUv.add(vec2(SPLAT_TEXEL, SPLAT_TEXEL)))
    .toVar()

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function decodeCell(sample: any) {
    const pk = sample.r.mul(255.0).add(0.5).floor()
    const p = pk.div(16.0).floor().toVar()
    const s = pk.sub(p.mul(16.0)).toVar()
    return { p, s, blend: sample.b }
  }

  const d00 = decodeCell(s00)
  const d10 = decodeCell(s10)
  const d01 = decodeCell(s01)
  const d11 = decodeCell(s11)

  // Weight of a decoded cell toward the given palette index.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function cellWeight(d: any, targetIdx: any) {
    const pMatch = step(abs(d.p.sub(targetIdx)), float(0.5))
    const sMatch = step(abs(d.s.sub(targetIdx)), float(0.5))
    return pMatch.mul(d.blend.oneMinus()).add(sMatch.mul(d.blend))
  }

  // Nearest cell (pixel's actual cell): select one of the 4 taps by fracUv.
  const selX = step(float(0.5), fracUv.x)
  const selY = step(float(0.5), fracUv.y)
  const pIdxF = mix(
    mix(d00.p, d10.p, selX),
    mix(d01.p, d11.p, selX),
    selY
  ).toVar()
  const sIdxF = mix(
    mix(d00.s, d10.s, selX),
    mix(d01.s, d11.s, selX),
    selY
  ).toVar()
  const nearestBlend = mix(
    mix(d00.blend, d10.blend, selX),
    mix(d01.blend, d11.blend, selX),
    selY
  )

  const w00 = fracUv.x.oneMinus().mul(fracUv.y.oneMinus())
  const w10 = fracUv.x.mul(fracUv.y.oneMinus())
  const w01 = fracUv.x.oneMinus().mul(fracUv.y)
  const w11 = fracUv.x.mul(fracUv.y)

  const pW = cellWeight(d00, pIdxF)
    .mul(w00)
    .add(cellWeight(d10, pIdxF).mul(w10))
    .add(cellWeight(d01, pIdxF).mul(w01))
    .add(cellWeight(d11, pIdxF).mul(w11))
  const sW = cellWeight(d00, sIdxF)
    .mul(w00)
    .add(cellWeight(d10, sIdxF).mul(w10))
    .add(cellWeight(d01, sIdxF).mul(w01))
    .add(cellWeight(d11, sIdxF).mul(w11))

  const totalW = pW.add(sW)
  const hasWeight = step(float(0.01), totalW)
  const interpBlend = sW.div(max(totalW, float(1e-4)))
  const blend = mix(nearestBlend, interpBlend, hasWeight).toVar()

  const fLocalUv = uv()
  const fUvDx = dFdx(fLocalUv)
  const fUvDy = dFdy(fLocalUv)

  // Compute atlas UV + texture gradients for a given slot index.
  // idxF: float 0..MAX_PALETTE-1. tileScale: float.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function slotUv(idxF: any, tileScale: any) {
    const slotCol = idxF.mod(float(ATLAS_GRID))
    const slotRow = idxF.div(float(ATLAS_GRID)).floor()
    const slotOffset = vec2(slotCol, slotRow).mul(GRID_INV)
    const tiled = fLocalUv.mul(tileScale)
    const atlasUv = fract(tiled)
      .mul(SUBTEX_NORM)
      .add(slotOffset)
      .add(BORDER_NORM)
    const gx = fUvDx.mul(tileScale).mul(SUBTEX_NORM)
    const gy = fUvDy.mul(tileScale).mul(SUBTEX_NORM)
    return { atlasUv, gx, gy }
  }

  const tileP = uTileScales.element(int(pIdxF)).toVar()
  const tileS = uTileScales.element(int(sIdxF)).toVar()
  const pSlot = slotUv(pIdxF, tileP)
  const sSlot = slotUv(sIdxF, tileS)

  function sampleAtlasAt(
    atlasTex: ShaderNodeObject<TextureNode>,
    slot: ReturnType<typeof slotUv>
  ) {
    return (
      atlasTex.sample(slot.atlasUv) as unknown as ShaderNodeObject<TextureNode>
    ).grad(slot.gx, slot.gy)
  }

  // ─── Color node ─────────────────────────────────────────
  const colorNode = Fn(() => {
    const cP = sampleAtlasAt(diffAtlasTex, pSlot).rgb
    const cS = sampleAtlasAt(diffAtlasTex, sSlot).rgb
    const blended = mix(cP, cS, blend)

    if (!brush) return vec4(blended, 1.0)

    const b = blended.toVar()
    const gridActive = smoothstep(float(0.49), float(0.51), brush.gridVisible)

    const gridCoords = fLocalUv.mul(64.0)
    const grid1 = abs(fract(gridCoords.sub(0.5)).sub(0.5)).div(
      fwidth(gridCoords)
    )
    const line1 = float(1).sub(min(min(grid1.x, grid1.y), float(1)))
    const grid64 = abs(fract(fLocalUv.sub(0.5)).sub(0.5)).div(fwidth(fLocalUv))
    const line64 = float(1).sub(min(min(grid64.x, grid64.y), float(1)))
    const regionCoords = vWorldXZ.add(32.0).div(1024.0)
    const gridRegion = abs(fract(regionCoords.sub(0.5)).sub(0.5)).div(
      fwidth(regionCoords)
    )
    const lineRegion = float(1).sub(
      min(min(gridRegion.x, gridRegion.y), float(1))
    )

    b.assign(mix(b, mix(b, vec3(0, 0, 0), line1.mul(0.3)), gridActive))
    b.assign(mix(b, mix(b, vec3(1, 0, 0), line64), gridActive))
    b.assign(mix(b, vec3(0.886, 0.725, 0.231), lineRegion.mul(gridActive)))

    const bDist = distance(vWorldXZ, vec2(brush.center))
    const ringWidth = max(float(0.5), float(brush.radius).mul(0.1))
    const innerRadius = float(brush.radius).sub(ringWidth)
    const inRing = smoothstep(innerRadius.sub(0.1), innerRadius, bDist).mul(
      float(1).sub(
        smoothstep(float(brush.radius), float(brush.radius).add(0.1), bDist)
      )
    )
    const heightColor = mix(
      vec3(1.0, 0.3, 0.3),
      mix(
        vec3(0.3, 1.0, 0.3),
        vec3(0.3, 0.6, 1.0),
        smoothstep(float(1.49), float(1.51), brush.raise)
      ),
      smoothstep(float(0.49), float(0.51), brush.raise)
    )
    const brushColor = mix(
      heightColor,
      vec3(1.0, 0.7, 0.2),
      smoothstep(float(0.49), float(0.51), brush.toolMode)
    )
    const brushAlpha = inRing
      .mul(0.35)
      .mul(smoothstep(float(0.49), float(0.51), brush.active))
    b.assign(mix(b, brushColor, brushAlpha))

    return vec4(b, 1.0)
  })()

  // ─── Normal node ────────────────────────────────────────
  const normalNode = normAtlasTex
    ? Fn(() => {
        const nP = sampleAtlasAt(normAtlasTex, pSlot).xyz.mul(2.0).sub(1.0)
        const nS = sampleAtlasAt(normAtlasTex, sSlot).xyz.mul(2.0).sub(1.0)
        const tangentNormal = mix(nP, nS, blend).normalize()
        return TBNViewMatrix.mul(tangentNormal).normalize()
      })()
    : undefined

  // ─── ORM node ───────────────────────────────────────────
  const ormBlended = ormAtlasTex
    ? Fn(() => {
        const oP = sampleAtlasAt(ormAtlasTex, pSlot).rgb
        const oS = sampleAtlasAt(ormAtlasTex, sSlot).rgb
        return mix(oP, oS, blend)
      })()
    : null
  const roughnessNode = ormBlended ? ormBlended.g : undefined
  const metalnessNode = ormBlended ? ormBlended.b : undefined
  const aoNode = ormBlended ? ormBlended.r : undefined

  const mat = new MeshStandardNodeMaterial()
  mat.roughness = 1.0
  mat.metalness = 0.0
  mat.envMapIntensity = 0

  mat.positionNode = vertexNode
  mat.colorNode = colorNode
  if (normalNode) mat.normalNode = normalNode
  if (roughnessNode) mat.roughnessNode = roughnessNode
  if (metalnessNode) mat.metalnessNode = metalnessNode
  if (aoNode) mat.aoNode = aoNode

  mat.userData.uniforms = {
    splatMap: splatTex,
    diffuseAtlas: diffAtlasTex,
    ...(normAtlasTex ? { normalAtlas: normAtlasTex } : {}),
    ...(ormAtlasTex ? { ormAtlas: ormAtlasTex } : {}),
    uTileScales,
    ...(brush
      ? {
          brushCenter: brush.center,
          brushRadius: brush.radius,
          brushActive: brush.active,
          brushRaise: brush.raise,
          brushToolMode: brush.toolMode,
          gridVisible: brush.gridVisible,
        }
      : {}),
  }

  return mat
}
