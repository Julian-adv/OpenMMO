// makeSplatStandardMaterial.ts — TSL/WebGPU version
// Kept lightweight for WebGL2 fallback compatibility (≤ ~10 texture bindings)
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

  // Brush overlay — world position reconstructed from UV + tile origin
  const uBrushCenter = uniform(new THREE.Vector2(0, 0))
  const uBrushRadius = uniform(3.0)
  const uBrushActive = uniform(0.0)
  const uBrushRaise = uniform(1.0)
  const uBrushToolMode = uniform(0.0)
  const uGridVisible = uniform(0.0)
  const uTileOrigin = uniform(new THREE.Vector2(0, 0))
  const uTileSize = uniform(64.0)

  // ─── Texture nodes ───────────────────────────────────
  // Fragment textures: 1 splat + 4 diffuse + 4 normal = 9
  // Plus internal (shadow map, envBRDF, etc.) stays within WebGL2 limit of 16
  const splatTex = texture(splatMap)
  const diffTex0 = texture(layers[0].map)
  const diffTex1 = texture(layers[1].map)
  const diffTex2 = texture(layers[2].map)
  const diffTex3 = texture(layers[3].map)

  const hasN = layers.some((l) => !!l.normalMap)

  const placeholderTex = new THREE.DataTexture(
    new Uint8Array([128, 128, 255, 255]),
    1,
    1,
    THREE.RGBAFormat
  )
  placeholderTex.needsUpdate = true

  const normTex0 = hasN ? texture(layers[0].normalMap ?? placeholderTex) : null
  const normTex1 = hasN ? texture(layers[1].normalMap ?? placeholderTex) : null
  const normTex2 = hasN ? texture(layers[2].normalMap ?? placeholderTex) : null
  const normTex3 = hasN ? texture(layers[3].normalMap ?? placeholderTex) : null

  // Caustics uniforms — not connected to shader nodes for now (WebGL compat),
  // but kept in userData so external code doesn't break.
  const causticsUniforms = {
    causticsMap: { value: null as THREE.Texture | null },
    causticsTime: { value: 0.0 },
    causticsStrength: { value: 0.275 },
    causticsScale: { value: 0.15 },
    waterLevel: { value: 0.01 },
  }

  // ─── Helper: normalized splat weights ─────────────
  const getWeights = Fn(([uvCoord]: [ReturnType<typeof vec2>]) => {
    const w = splatTex.uv(uvCoord).toVar()
    const wSum = w.r.add(w.g).add(w.b).add(w.a)
    w.assign(mix(w, w.div(wSum), smoothstep(float(0), float(1e-5), wSum)))
    return w
  })

  // ─── Color node (albedo blending + overlays) ──────
  const colorNode = Fn(() => {
    const localUv = uv()
    const splatUv = localUv.mul(uSplatScale)
    const weights = getWeights(splatUv)

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

    // Grid visualization (UV-based, no world position needed)
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

    // Brush overlay — reconstruct world XZ from UV + tile origin
    // PlaneGeometry rotated to XZ: uv.x → X, uv.y → -Z
    const worldXZ = vec2(
      uTileOrigin.x.add(localUv.x.sub(0.5).mul(uTileSize)),
      uTileOrigin.y.add(float(0.5).sub(localUv.y).mul(uTileSize))
    )
    const bDist = distance(worldXZ, vec2(uBrushCenter))
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
        const w = getWeights(localUv.mul(uSplatScale))

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

  // ─── Build material ────────────────────────────────
  const mat = new MeshStandardNodeMaterial()
  mat.roughness = 0.85
  mat.metalness = 0.0
  mat.envMapIntensity = 0

  mat.colorNode = colorNode
  if (normalNode) mat.normalNode = normalNode

  // Store uniforms for external access
  mat.userData.uniforms = {
    splatMap: splatTex,
    brushCenter: uBrushCenter,
    brushRadius: uBrushRadius,
    brushActive: uBrushActive,
    brushRaise: uBrushRaise,
    brushToolMode: uBrushToolMode,
    gridVisible: uGridVisible,
    tileOrigin: uTileOrigin,
    tileSize: uTileSize,
    ...causticsUniforms,
  }

  return mat
}
