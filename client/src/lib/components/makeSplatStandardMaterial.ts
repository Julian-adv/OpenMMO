// makeSplatStandardMaterial.ts — TSL/WebGPU version (atlas-based)
import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import {
  Fn,
  uniform,
  texture,
  uv,
  vec2,
  vec3,
  vec4,
  float,
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
import { ATLAS_BORDER, type SplatAtlasSet } from '../utils/splatLayerLoader'

export type SplatLayer = {
  map: THREE.Texture // Albedo (sRGB)
  normalMap?: THREE.Texture // Normal (Linear)
  orm?: THREE.Texture // ORM: R=AO, G=Roughness, B=Metallic (Linear)
  tile: number
}

export type SplatParams = {
  atlas: SplatAtlasSet
  tileScales: [number, number, number, number]
  splatMap: THREE.Texture // RGBA weight map (R=layer0, G=layer1, B=layer2, A=layer3)
  splatScale?: number // UV scale of the splat map (default 1)
  sharedBrushUniforms?: SplatBrushUniforms // Reuse brush/grid uniforms across materials
  /** Include grid/brush editor overlay in the shader. Default false.
   *  When false, the generated WGSL is significantly smaller → faster pipeline compilation.
   *  Only set to true when the map editor is active. */
  includeEditorOverlay?: boolean
}

/** Shared brush/grid uniform nodes — create once, pass to every per-tile material. */
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

// ─── Atlas quadrant offsets (2×2 layout with border padding) ──
// Each slot is (srcSize + 2*ATLAS_BORDER). Slot occupies exactly 0.5 of atlas.
// Sub-texture starts at ATLAS_BORDER pixels into each slot.
// [0]=TL, [1]=TR, [2]=BL, [3]=BR — matches buildAtlasTexture layout
const QUAD_OFFSETS = [vec2(0, 0), vec2(0.5, 0), vec2(0, 0.5), vec2(0.5, 0.5)]

export function makeSplatStandardMaterial({
  atlas,
  tileScales,
  splatMap,
  splatScale = 1,
  sharedBrushUniforms,
  includeEditorOverlay = false,
}: SplatParams) {
  // Prepare splat map
  splatMap.wrapS = splatMap.wrapT = THREE.RepeatWrapping
  splatMap.anisotropy = 8
  splatMap.minFilter = THREE.LinearMipMapLinearFilter
  splatMap.magFilter = THREE.LinearFilter
  splatMap.needsUpdate = true

  // ─── Scalar uniforms ─────────────────────────────────
  const uTile0 = uniform(tileScales[0])
  const uTile1 = uniform(tileScales[1])
  const uTile2 = uniform(tileScales[2])
  const uTile3 = uniform(tileScales[3])
  const uSplatScale = uniform(splatScale)

  // Brush overlay uniforms — only created when editor overlay is enabled
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

  // ─── Atlas texture nodes ──────────────────────────────
  // 1 splat + 1 diffuse atlas + 1 normal atlas + 1 ORM atlas = 4 textures
  // (vs. 13 before) — leaves plenty of room for shadow maps etc.
  const splatTex = texture(splatMap)
  const diffAtlasTex = texture(atlas.diffuseAtlas)
  const normAtlasTex = atlas.normalAtlas ? texture(atlas.normalAtlas) : null
  const ormAtlasTex = atlas.ormAtlas ? texture(atlas.ormAtlas) : null

  // ─── Varyings: world position from vertex ─────────
  const vUvSplat = varying(vec2(0), 'v_uvSplat')
  const vWorldXZ = varying(vec2(0), 'v_worldXZ')
  const vWorldY = varying(float(0), 'v_worldY')

  // ─── Helper: normalized splat weights ─────────────
  const getWeights = Fn(([uvCoord]: [ReturnType<typeof vec2>]) => {
    const w = splatTex.sample(uvCoord).toVar()
    const wSum = w.r.add(w.g).add(w.b).add(w.a)
    w.assign(mix(w, w.div(wSum), smoothstep(float(0), float(1e-5), wSum)))
    return w
  })

  // ─── Helper: sample atlas with correct tiling + mipmapping ──
  // Uses fract() for manual repeat + .grad() with continuous derivatives
  // to avoid the mipmap seam that fract() discontinuity would cause.
  // UV is mapped to the inner sub-texture region, skipping the border padding.
  //
  // Atlas layout per slot: [BORDER | srcTexture | BORDER]
  // slotSize = srcSize + 2*BORDER, atlas = slotSize*2 per axis
  // borderNorm = BORDER / (slotSize * 2)  — border in normalized atlas UV
  // subTexNorm = srcSize / (slotSize * 2) — sub-texture extent in atlas UV
  // Since slotSize*2 = atlas width, and each slot = 0.5 of atlas:
  //   borderInQuad = BORDER / slotSize (within the 0.5 quadrant)
  //   subTexInQuad = srcSize / slotSize
  // We assume srcSize=1024 (the dominant case).
  const _srcSize = 1024
  const _slotSize = _srcSize + ATLAS_BORDER * 2
  const _borderNorm = ATLAS_BORDER / (_slotSize * 2) // border in full atlas UV
  const _subTexNorm = _srcSize / (_slotSize * 2) // sub-texture size in full atlas UV

  // ─── Vertex position node (adds varyings) ─────────
  const vertexNode = Fn(() => {
    const localUv = uv()
    vUvSplat.assign(localUv.mul(uSplatScale))
    const worldPos4 = modelWorldMatrix.mul(vec4(positionLocal, 1.0))
    vWorldXZ.assign(worldPos4.xz)
    vWorldY.assign(worldPos4.y)
    return positionLocal
  })()

  // ─── Shared fragment inputs (computed once, reused across all nodes) ──
  // TSL deduplicates node references within the same shader, so computing
  // weights + UV derivatives once and reusing the results avoids redundant
  // getWeights/dFdx/dFdy evaluations across color/normal/ORM nodes.
  const fLocalUv = uv()
  const fWeights = getWeights(vUvSplat)
  const fUvDx = dFdx(fLocalUv)
  const fUvDy = dFdy(fLocalUv)

  // ─── Pre-compute per-layer atlas UV + gradients ──────────
  // atlasUv/gx/gy are identical for diffuse/normal/ORM at the same layer.
  // Sharing these TSL nodes across all three atlas textures avoids redundant
  // fract/mul/add and gradient computations in the generated WGSL.
  const uTileScales = [uTile0, uTile1, uTile2, uTile3]
  const layerAtlas = uTileScales.map((tileScale, i) => {
    const tiledUv = fLocalUv.mul(tileScale)
    const atlasUv = fract(tiledUv)
      .mul(_subTexNorm)
      .add(QUAD_OFFSETS[i])
      .add(_borderNorm)
    const gx = fUvDx.mul(tileScale).mul(_subTexNorm)
    const gy = fUvDy.mul(tileScale).mul(_subTexNorm)
    return { atlasUv, gx, gy }
  })

  function sampleAtlasLayer(
    atlasTex: ShaderNodeObject<TextureNode>,
    layerIdx: number
  ) {
    const { atlasUv, gx, gy } = layerAtlas[layerIdx]
    return (
      atlasTex.sample(atlasUv) as unknown as ShaderNodeObject<TextureNode>
    ).grad(gx, gy)
  }

  // ─── Color node (albedo blending + editor overlays) ──────
  const colorNode = Fn(() => {
    const c0 = sampleAtlasLayer(diffAtlasTex, 0).rgb
    const c1 = sampleAtlasLayer(diffAtlasTex, 1).rgb
    const c2 = sampleAtlasLayer(diffAtlasTex, 2).rgb
    const c3 = sampleAtlasLayer(diffAtlasTex, 3).rgb
    const blended = c0
      .mul(fWeights.r)
      .add(c1.mul(fWeights.g))
      .add(c2.mul(fWeights.b))
      .add(c3.mul(fWeights.a))

    if (!brush) {
      return vec4(blended, 1.0)
    }

    // Editor grid + brush overlay — only compiled into WGSL when editor is active.
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

  // ─── Normal node (splat-blended normals from atlas) ──────────
  const normalNode = normAtlasTex
    ? Fn(() => {
        const n0 = sampleAtlasLayer(normAtlasTex, 0)
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(fWeights.r)
        const n1 = sampleAtlasLayer(normAtlasTex, 1)
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(fWeights.g)
        const n2 = sampleAtlasLayer(normAtlasTex, 2)
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(fWeights.b)
        const n3 = sampleAtlasLayer(normAtlasTex, 3)
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(fWeights.a)
        const tangentNormal = n0.add(n1).add(n2).add(n3).normalize()
        return TBNViewMatrix.mul(tangentNormal).normalize()
      })()
    : undefined

  // ─── ORM node (single pass: sample atlas once, extract AO/roughness/metalness) ──
  // Previously roughness, metalness, and AO were separate Fn()s each sampling
  // the ORM atlas 4 times (12 total). Merged into one Fn with 4 samples.
  const ormBlended = ormAtlasTex
    ? Fn(() => {
        const o0 = sampleAtlasLayer(ormAtlasTex, 0).rgb
        const o1 = sampleAtlasLayer(ormAtlasTex, 1).rgb
        const o2 = sampleAtlasLayer(ormAtlasTex, 2).rgb
        const o3 = sampleAtlasLayer(ormAtlasTex, 3).rgb
        return o0
          .mul(fWeights.r)
          .add(o1.mul(fWeights.g))
          .add(o2.mul(fWeights.b))
          .add(o3.mul(fWeights.a))
      })()
    : null
  const roughnessNode = ormBlended ? ormBlended.g : undefined
  const metalnessNode = ormBlended ? ormBlended.b : undefined
  const aoNode = ormBlended ? ormBlended.r : undefined

  // ─── Build material ────────────────────────────────
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

  // Store uniforms for external access (atlas textures swappable per-tile)
  mat.userData.uniforms = {
    splatMap: splatTex,
    diffuseAtlas: diffAtlasTex,
    ...(normAtlasTex ? { normalAtlas: normAtlasTex } : {}),
    ...(ormAtlasTex ? { ormAtlas: ormAtlasTex } : {}),
    uTile0,
    uTile1,
    uTile2,
    uTile3,
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
