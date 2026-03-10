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
import type Node from 'three/src/nodes/core/Node.js'
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

  // Brush overlay — shared across materials when provided
  const uBrushCenter =
    sharedBrushUniforms?.brushCenter ?? uniform(new THREE.Vector2(0, 0))
  const uBrushRadius = sharedBrushUniforms?.brushRadius ?? uniform(3.0)
  const uBrushActive = sharedBrushUniforms?.brushActive ?? uniform(0.0)
  const uBrushRaise = sharedBrushUniforms?.brushRaise ?? uniform(1.0)
  const uBrushToolMode = sharedBrushUniforms?.brushToolMode ?? uniform(0.0)
  const uGridVisible = sharedBrushUniforms?.gridVisible ?? uniform(0.0)

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

  function sampleAtlas(
    atlasTex: ShaderNodeObject<TextureNode>,
    baseUv: ReturnType<typeof uv>,
    tileScale: ReturnType<typeof uniform>,
    quadOffset: ShaderNodeObject<Node>,
    dUVdx: ReturnType<typeof dFdx>,
    dUVdy: ReturnType<typeof dFdy>
  ) {
    const tiledUv = baseUv.mul(tileScale)
    // Map fract() [0,1) to the sub-texture region within the quadrant,
    // offset past the border padding.
    const atlasUv = fract(tiledUv)
      .mul(_subTexNorm)
      .add(quadOffset)
      .add(_borderNorm)
    // Gradients scaled to sub-texture size in atlas space
    const gx = dUVdx.mul(tileScale).mul(_subTexNorm)
    const gy = dUVdy.mul(tileScale).mul(_subTexNorm)
    return (
      atlasTex.sample(atlasUv) as unknown as ShaderNodeObject<TextureNode>
    ).grad(gx, gy)
  }

  // ─── Vertex position node (adds varyings) ─────────
  const vertexNode = Fn(() => {
    const localUv = uv()
    vUvSplat.assign(localUv.mul(uSplatScale))
    const worldPos4 = modelWorldMatrix.mul(vec4(positionLocal, 1.0))
    vWorldXZ.assign(worldPos4.xz)
    vWorldY.assign(worldPos4.y)
    return positionLocal
  })()

  // ─── Color node (albedo blending + overlays) ──────
  const colorNode = Fn(() => {
    const localUv = uv()
    const weights = getWeights(vUvSplat)
    const uvDx = dFdx(localUv)
    const uvDy = dFdy(localUv)

    const c0 = sampleAtlas(
      diffAtlasTex,
      localUv,
      uTile0,
      QUAD_OFFSETS[0],
      uvDx,
      uvDy
    ).rgb
    const c1 = sampleAtlas(
      diffAtlasTex,
      localUv,
      uTile1,
      QUAD_OFFSETS[1],
      uvDx,
      uvDy
    ).rgb
    const c2 = sampleAtlas(
      diffAtlasTex,
      localUv,
      uTile2,
      QUAD_OFFSETS[2],
      uvDx,
      uvDy
    ).rgb
    const c3 = sampleAtlas(
      diffAtlasTex,
      localUv,
      uTile3,
      QUAD_OFFSETS[3],
      uvDx,
      uvDy
    ).rgb
    const blended = c0
      .mul(weights.r)
      .add(c1.mul(weights.g))
      .add(c2.mul(weights.b))
      .add(c3.mul(weights.a))
      .toVar()

    // Grid visualization
    const gridCoords = localUv.mul(64.0)
    const grid1 = abs(fract(gridCoords.sub(0.5)).sub(0.5)).div(
      fwidth(gridCoords)
    )
    const line1 = float(1).sub(min(min(grid1.x, grid1.y), float(1)))
    const grid64 = abs(fract(localUv.sub(0.5)).sub(0.5)).div(fwidth(localUv))
    const line64 = float(1).sub(min(min(grid64.x, grid64.y), float(1)))

    // Region boundary grid (16 tiles = 1024 world units, offset by half tile)
    const regionCoords = vWorldXZ.add(32.0).div(1024.0)
    const gridRegion = abs(fract(regionCoords.sub(0.5)).sub(0.5)).div(
      fwidth(regionCoords)
    )
    const lineRegion = float(1).sub(
      min(min(gridRegion.x, gridRegion.y), float(1))
    )

    const gridActive = smoothstep(float(0.49), float(0.51), uGridVisible)
    blended.assign(
      mix(blended, mix(blended, vec3(0, 0, 0), line1.mul(0.3)), gridActive)
    )
    blended.assign(
      mix(blended, mix(blended, vec3(1, 0, 0), line64), gridActive)
    )
    blended.assign(
      mix(blended, vec3(0.886, 0.725, 0.231), lineRegion.mul(gridActive))
    )

    // Brush overlay
    const bDist = distance(vWorldXZ, vec2(uBrushCenter))
    const ringWidth = max(float(0.5), float(uBrushRadius).mul(0.1))
    const innerRadius = float(uBrushRadius).sub(ringWidth)
    const inRing = smoothstep(innerRadius.sub(0.1), innerRadius, bDist).mul(
      float(1).sub(
        smoothstep(float(uBrushRadius), float(uBrushRadius).add(0.1), bDist)
      )
    )

    const splatColor = vec3(1.0, 0.7, 0.2)
    const flattenColor = vec3(0.3, 0.6, 1.0)
    const raiseColor = vec3(0.3, 1.0, 0.3)
    const lowerColor = vec3(1.0, 0.3, 0.3)

    const heightColor = mix(
      lowerColor,
      mix(
        raiseColor,
        flattenColor,
        smoothstep(float(1.49), float(1.51), uBrushRaise)
      ),
      smoothstep(float(0.49), float(0.51), uBrushRaise)
    )
    const brushColor = mix(
      heightColor,
      splatColor,
      smoothstep(float(0.49), float(0.51), uBrushToolMode)
    )

    const brushAlpha = inRing
      .mul(0.35)
      .mul(smoothstep(float(0.49), float(0.51), uBrushActive))
    blended.assign(mix(blended, brushColor, brushAlpha))

    return vec4(blended, 1.0)
  })()

  // ─── Normal node (splat-blended normals from atlas) ──────────
  const normalNode = normAtlasTex
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)
        const uvDx = dFdx(localUv)
        const uvDy = dFdy(localUv)

        const n0 = sampleAtlas(
          normAtlasTex,
          localUv,
          uTile0,
          QUAD_OFFSETS[0],
          uvDx,
          uvDy
        )
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.r)
        const n1 = sampleAtlas(
          normAtlasTex,
          localUv,
          uTile1,
          QUAD_OFFSETS[1],
          uvDx,
          uvDy
        )
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.g)
        const n2 = sampleAtlas(
          normAtlasTex,
          localUv,
          uTile2,
          QUAD_OFFSETS[2],
          uvDx,
          uvDy
        )
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.b)
        const n3 = sampleAtlas(
          normAtlasTex,
          localUv,
          uTile3,
          QUAD_OFFSETS[3],
          uvDx,
          uvDy
        )
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.a)

        const tangentNormal = n0.add(n1).add(n2).add(n3).normalize()
        // Convert tangent-space normal to view-space via TBN matrix.
        // mat.normalNode is used directly as normalView, so we must provide
        // a view-space normal — not a tangent-space one.
        return TBNViewMatrix.mul(tangentNormal).normalize()
      })()
    : undefined

  // ─── Roughness node (ORM atlas G channel) ───────────────
  const roughnessNode = ormAtlasTex
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)
        const uvDx = dFdx(localUv)
        const uvDy = dFdy(localUv)

        const r0 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile0,
          QUAD_OFFSETS[0],
          uvDx,
          uvDy
        ).g
        const r1 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile1,
          QUAD_OFFSETS[1],
          uvDx,
          uvDy
        ).g
        const r2 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile2,
          QUAD_OFFSETS[2],
          uvDx,
          uvDy
        ).g
        const r3 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile3,
          QUAD_OFFSETS[3],
          uvDx,
          uvDy
        ).g

        return r0.mul(w.r).add(r1.mul(w.g)).add(r2.mul(w.b)).add(r3.mul(w.a))
      })()
    : undefined

  // ─── Metalness node (ORM atlas B channel) ───────────────
  const metalnessNode = ormAtlasTex
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)
        const uvDx = dFdx(localUv)
        const uvDy = dFdy(localUv)

        const m0 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile0,
          QUAD_OFFSETS[0],
          uvDx,
          uvDy
        ).b
        const m1 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile1,
          QUAD_OFFSETS[1],
          uvDx,
          uvDy
        ).b
        const m2 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile2,
          QUAD_OFFSETS[2],
          uvDx,
          uvDy
        ).b
        const m3 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile3,
          QUAD_OFFSETS[3],
          uvDx,
          uvDy
        ).b

        return m0.mul(w.r).add(m1.mul(w.g)).add(m2.mul(w.b)).add(m3.mul(w.a))
      })()
    : undefined

  // ─── AO node (ORM atlas R channel) ──────────────────────
  const aoNode = ormAtlasTex
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)
        const uvDx = dFdx(localUv)
        const uvDy = dFdy(localUv)

        const ao0 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile0,
          QUAD_OFFSETS[0],
          uvDx,
          uvDy
        ).r
        const ao1 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile1,
          QUAD_OFFSETS[1],
          uvDx,
          uvDy
        ).r
        const ao2 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile2,
          QUAD_OFFSETS[2],
          uvDx,
          uvDy
        ).r
        const ao3 = sampleAtlas(
          ormAtlasTex,
          localUv,
          uTile3,
          QUAD_OFFSETS[3],
          uvDx,
          uvDy
        ).r

        return ao0
          .mul(w.r)
          .add(ao1.mul(w.g))
          .add(ao2.mul(w.b))
          .add(ao3.mul(w.a))
      })()
    : undefined

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
    brushCenter: uBrushCenter,
    brushRadius: uBrushRadius,
    brushActive: uBrushActive,
    brushRaise: uBrushRaise,
    brushToolMode: uBrushToolMode,
    gridVisible: uGridVisible,
  }

  return mat
}
