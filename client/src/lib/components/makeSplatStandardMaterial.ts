// makeSplatStandardMaterial.ts — TSL/WebGPU version
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
  TBNViewMatrix,
} from 'three/tsl'

export type SplatLayer = {
  map: THREE.Texture // Albedo (sRGB)
  normalMap?: THREE.Texture // Normal (Linear)
  orm?: THREE.Texture // ORM: R=AO, G=Roughness, B=Metallic (Linear)
  tile: number
}

export type SplatParams = {
  layers: [SplatLayer, SplatLayer, SplatLayer, SplatLayer] // RGBA order
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

export function makeSplatStandardMaterial({
  layers,
  splatMap,
  splatScale = 1,
  sharedBrushUniforms,
}: SplatParams) {
  // Recommended common texture settings
  const prepare = (t: THREE.Texture, isColor = false) => {
    t.wrapS = t.wrapT = THREE.RepeatWrapping
    t.anisotropy = 8
    if (isColor) t.colorSpace = THREE.SRGBColorSpace
    t.needsUpdate = true
  }

  layers.forEach((l) => prepare(l.map, true))
  prepare(splatMap, false)
  splatMap.minFilter = THREE.LinearMipMapLinearFilter
  splatMap.magFilter = THREE.LinearFilter

  // ─── Scalar uniforms ─────────────────────────────────
  const uTile0 = uniform(layers[0].tile)
  const uTile1 = uniform(layers[1].tile)
  const uTile2 = uniform(layers[2].tile)
  const uTile3 = uniform(layers[3].tile)
  const uSplatScale = uniform(splatScale)

  // Brush overlay — shared across materials when provided
  const uBrushCenter =
    sharedBrushUniforms?.brushCenter ?? uniform(new THREE.Vector2(0, 0))
  const uBrushRadius = sharedBrushUniforms?.brushRadius ?? uniform(3.0)
  const uBrushActive = sharedBrushUniforms?.brushActive ?? uniform(0.0)
  const uBrushRaise = sharedBrushUniforms?.brushRaise ?? uniform(1.0)
  const uBrushToolMode = sharedBrushUniforms?.brushToolMode ?? uniform(0.0)
  const uGridVisible = sharedBrushUniforms?.gridVisible ?? uniform(0.0)

  // ─── Texture nodes ───────────────────────────────────
  // Fragment: 1 splat + 4 diffuse + 4 normal + 4 ORM = 13
  // Plus internal (shadow map, envBRDF, etc.) stays within WebGPU limit of 16
  const splatTex = texture(splatMap)
  const diffTex0 = texture(layers[0].map)
  const diffTex1 = texture(layers[1].map)
  const diffTex2 = texture(layers[2].map)
  const diffTex3 = texture(layers[3].map)

  const hasN = layers.some((l) => !!l.normalMap)
  const hasORM = false // TEMP: disabled to free texture slots for PointLight shadow
  // const hasORM = layers.some((l) => !!l.orm)

  // Placeholder texture for missing layers
  const placeholderTex = new THREE.DataTexture(
    new Uint8Array([128, 128, 255, 255]),
    1,
    1,
    THREE.RGBAFormat
  )
  placeholderTex.needsUpdate = true
  const placeholderORM = new THREE.DataTexture(
    new Uint8Array([255, 255, 0, 255]),
    1,
    1,
    THREE.RGBAFormat
  )
  placeholderORM.needsUpdate = true

  const normTex0 = hasN ? texture(layers[0].normalMap ?? placeholderTex) : null
  const normTex1 = hasN ? texture(layers[1].normalMap ?? placeholderTex) : null
  const normTex2 = hasN ? texture(layers[2].normalMap ?? placeholderTex) : null
  const normTex3 = hasN ? texture(layers[3].normalMap ?? placeholderTex) : null

  const ormTex0 = hasORM ? texture(layers[0].orm ?? placeholderORM) : null
  const ormTex1 = hasORM ? texture(layers[1].orm ?? placeholderORM) : null
  const ormTex2 = hasORM ? texture(layers[2].orm ?? placeholderORM) : null
  const ormTex3 = hasORM ? texture(layers[3].orm ?? placeholderORM) : null

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

    const c0 = diffTex0.sample(localUv.mul(uTile0)).rgb
    const c1 = diffTex1.sample(localUv.mul(uTile1)).rgb
    const c2 = diffTex2.sample(localUv.mul(uTile2)).rgb
    const c3 = diffTex3.sample(localUv.mul(uTile3)).rgb
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

  // ─── Normal node (splat-blended normals) ──────────
  const normalNode = hasN
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)

        const n0 = normTex0!
          .sample(localUv.mul(uTile0))
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.r)
        const n1 = normTex1!
          .sample(localUv.mul(uTile1))
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.g)
        const n2 = normTex2!
          .sample(localUv.mul(uTile2))
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.b)
        const n3 = normTex3!
          .sample(localUv.mul(uTile3))
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

  // ─── Roughness node (ORM G channel) ───────────────
  const roughnessNode = hasORM
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)

        const r0 = ormTex0!.sample(localUv.mul(uTile0)).g
        const r1 = ormTex1!.sample(localUv.mul(uTile1)).g
        const r2 = ormTex2!.sample(localUv.mul(uTile2)).g
        const r3 = ormTex3!.sample(localUv.mul(uTile3)).g

        return r0.mul(w.r).add(r1.mul(w.g)).add(r2.mul(w.b)).add(r3.mul(w.a))
      })()
    : undefined

  // ─── Metalness node (ORM B channel) ───────────────
  const metalnessNode = hasORM
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)

        const m0 = ormTex0!.sample(localUv.mul(uTile0)).b
        const m1 = ormTex1!.sample(localUv.mul(uTile1)).b
        const m2 = ormTex2!.sample(localUv.mul(uTile2)).b
        const m3 = ormTex3!.sample(localUv.mul(uTile3)).b

        return m0.mul(w.r).add(m1.mul(w.g)).add(m2.mul(w.b)).add(m3.mul(w.a))
      })()
    : undefined

  // ─── AO node (ORM R channel) ──────────────────────
  const aoNode = hasORM
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)

        const ao0 = ormTex0!.sample(localUv.mul(uTile0)).r
        const ao1 = ormTex1!.sample(localUv.mul(uTile1)).r
        const ao2 = ormTex2!.sample(localUv.mul(uTile2)).r
        const ao3 = ormTex3!.sample(localUv.mul(uTile3)).r

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

  // Store uniforms for external access (layer textures swappable per-tile)
  mat.userData.uniforms = {
    splatMap: splatTex,
    diffTex0,
    diffTex1,
    diffTex2,
    diffTex3,
    ...(normTex0 ? { normTex0, normTex1, normTex2, normTex3 } : {}),
    ...(ormTex0 ? { ormTex0, ormTex1, ormTex2, ormTex3 } : {}),
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
