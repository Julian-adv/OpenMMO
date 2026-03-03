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
  sin,
  cos,
  smoothstep,
  mix,
  clamp,
  min,
  max,
  varying,
  positionLocal,
  modelWorldMatrix,
  fwidth,
  fract,
  abs,
  distance,
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
}

export function makeSplatStandardMaterial({
  layers,
  splatMap,
  splatScale = 1,
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

  // Brush overlay
  const uBrushCenter = uniform(new THREE.Vector2(0, 0))
  const uBrushRadius = uniform(3.0)
  const uBrushActive = uniform(0.0)
  const uBrushRaise = uniform(1.0)
  const uBrushToolMode = uniform(0.0)
  const uGridVisible = uniform(0.0)

  // Caustics
  const uCausticsTime = uniform(0.0)
  const uCausticsStrength = uniform(0.275)
  const uCausticsScale = uniform(0.15)
  const uWaterLevel = uniform(0.01)

  // ─── Texture nodes (direct THREE.Texture → TextureNode) ──
  const splatTex = texture(splatMap)
  const diffTex0 = texture(layers[0].map)
  const diffTex1 = texture(layers[1].map)
  const diffTex2 = texture(layers[2].map)
  const diffTex3 = texture(layers[3].map)

  const hasN = layers.some((l) => !!l.normalMap)
  const hasORM = layers.some((l) => !!l.orm)

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

  const caustTex = texture(placeholderTex) // will be updated via .value

  // ─── Varyings: world position from vertex ─────────
  const vUvSplat = varying(vec2(0), 'v_uvSplat')
  const vWorldXZ = varying(vec2(0), 'v_worldXZ')
  const vWorldY = varying(float(0), 'v_worldY')

  // ─── Helper: normalized splat weights ─────────────
  const getWeights = Fn(([uvCoord]: [ReturnType<typeof vec2>]) => {
    const w = splatTex.uv(uvCoord).toVar()
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

    const c0 = diffTex0.uv(localUv.mul(uTile0)).rgb
    const c1 = diffTex1.uv(localUv.mul(uTile1)).rgb
    const c2 = diffTex2.uv(localUv.mul(uTile2)).rgb
    const c3 = diffTex3.uv(localUv.mul(uTile3)).rgb
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

    const gridActive = smoothstep(float(0.49), float(0.51), uGridVisible)
    blended.assign(
      mix(blended, mix(blended, vec3(0, 0, 0), line1.mul(0.3)), gridActive)
    )
    blended.assign(
      mix(blended, mix(blended, vec3(1, 0, 0), line64), gridActive)
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
          .uv(localUv.mul(uTile0))
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.r)
        const n1 = normTex1!
          .uv(localUv.mul(uTile1))
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.g)
        const n2 = normTex2!
          .uv(localUv.mul(uTile2))
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.b)
        const n3 = normTex3!
          .uv(localUv.mul(uTile3))
          .xyz.mul(2.0)
          .sub(1.0)
          .mul(w.a)

        return n0.add(n1).add(n2).add(n3).normalize()
      })()
    : undefined

  // ─── Roughness node (ORM G channel) ───────────────
  const roughnessNode = hasORM
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)

        const r0 = ormTex0!.uv(localUv.mul(uTile0)).g
        const r1 = ormTex1!.uv(localUv.mul(uTile1)).g
        const r2 = ormTex2!.uv(localUv.mul(uTile2)).g
        const r3 = ormTex3!.uv(localUv.mul(uTile3)).g

        return r0.mul(w.r).add(r1.mul(w.g)).add(r2.mul(w.b)).add(r3.mul(w.a))
      })()
    : undefined

  // ─── Metalness node (ORM B channel) ───────────────
  const metalnessNode = hasORM
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)

        const m0 = ormTex0!.uv(localUv.mul(uTile0)).b
        const m1 = ormTex1!.uv(localUv.mul(uTile1)).b
        const m2 = ormTex2!.uv(localUv.mul(uTile2)).b
        const m3 = ormTex3!.uv(localUv.mul(uTile3)).b

        return m0.mul(w.r).add(m1.mul(w.g)).add(m2.mul(w.b)).add(m3.mul(w.a))
      })()
    : undefined

  // ─── AO node (ORM R channel) ──────────────────────
  const aoNode = hasORM
    ? Fn(() => {
        const localUv = uv()
        const w = getWeights(vUvSplat)

        const ao0 = ormTex0!.uv(localUv.mul(uTile0)).r
        const ao1 = ormTex1!.uv(localUv.mul(uTile1)).r
        const ao2 = ormTex2!.uv(localUv.mul(uTile2)).r
        const ao3 = ormTex3!.uv(localUv.mul(uTile3)).r

        return ao0
          .mul(w.r)
          .add(ao1.mul(w.g))
          .add(ao2.mul(w.b))
          .add(ao3.mul(w.a))
      })()
    : undefined

  // ─── Emissive node (caustics animation) ───────────
  const emissiveNode = Fn(() => {
    const t = uCausticsTime
    const sway = vec2(
      sin(t.mul(0.07).add(vWorldXZ.y.mul(2))),
      cos(t.mul(0.09).add(vWorldXZ.x.mul(2)))
    ).mul(0.02)

    const cUV1 = vWorldXZ
      .mul(uCausticsScale)
      .add(vec2(t.mul(0.03), t.mul(0.02)))
      .add(sway)
    const cUV2 = vWorldXZ
      .mul(float(uCausticsScale).mul(1.3))
      .sub(vec2(t.mul(0.02), t.mul(0.04)))
      .sub(sway.mul(1.3))

    const caustic = min(caustTex.uv(cUV1).r, caustTex.uv(cUV2).r)
    const underwaterDepth = clamp(
      float(uWaterLevel).sub(vWorldY).div(3),
      0.0,
      1.0
    )
    const caustFade = smoothstep(float(0.04), float(0.25), underwaterDepth).mul(
      float(1).sub(smoothstep(float(0.5), float(1), underwaterDepth))
    )

    const belowWater = smoothstep(
      float(uWaterLevel),
      float(uWaterLevel).sub(0.01),
      vWorldY
    )
    return vec3(caustic.mul(uCausticsStrength).mul(caustFade).mul(belowWater))
  })()

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
  mat.emissiveNode = emissiveNode

  // Store uniforms for external access (replaces mat.userData.shader pattern)
  mat.userData.uniforms = {
    splatMap: splatTex,
    brushCenter: uBrushCenter,
    brushRadius: uBrushRadius,
    brushActive: uBrushActive,
    brushRaise: uBrushRaise,
    brushToolMode: uBrushToolMode,
    gridVisible: uGridVisible,
    causticsMap: caustTex,
    causticsTime: uCausticsTime,
    causticsStrength: uCausticsStrength,
    causticsScale: uCausticsScale,
    waterLevel: uWaterLevel,
  }

  return mat
}
