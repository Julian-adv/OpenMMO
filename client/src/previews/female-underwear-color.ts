import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { meshIslands } from '../lib/utils/meshIslands'

export function femaleUnderwearColor(base: GLTF) {
  let body: THREE.SkinnedMesh | undefined
  base.scene.traverse((node) => {
    const association = base.parser.associations.get(node)
    if (
      node instanceof THREE.SkinnedMesh &&
      association?.meshes === 0 &&
      association.primitives === 0
    )
      body = node
  })
  if (!body) throw new Error('속옷의 기준 몸체를 찾지 못했습니다.')
  const mesh = body
  const source = mesh.geometry
  const original = source.getAttribute('color')
  const materials = (
    Array.isArray(mesh.material) ? mesh.material : [mesh.material]
  )
    .filter((material) => material instanceof THREE.MeshStandardMaterial)
    .map((material) => ({ material, vertexColors: material.vertexColors }))
  const { find, islands } = meshIslands(source, 1e6)
  const underwear = new Set(
    [...islands]
      .filter(([, { bounds }]) => bounds.min.y > 0.85 && bounds.max.y < 1.55)
      .map(([id]) => id)
  )
  const colors = new THREE.Float32BufferAttribute(
    new Float32Array(source.getAttribute('position').count * 3).fill(1),
    3
  )
  for (let i = 0; i < colors.count; i++) {
    if (underwear.has(find(i))) colors.setXYZ(i, 0.008, 0.009, 0.012)
  }
  const tint = { value: 0 }
  const skinTint = { value: 0 }
  const skinColor = {
    value: new THREE.Color().setRGB(
      194 / 255,
      151 / 255,
      125 / 255,
      THREE.SRGBColorSpace
    ),
  }
  for (const { material } of materials) {
    const compile = material.onBeforeCompile
    const cacheKey = material.customProgramCacheKey()
    material.customProgramCacheKey = () => `${cacheKey}:rogue-underwear`
    material.onBeforeCompile = (shader, renderer) => {
      compile.call(material, shader, renderer)
      shader.uniforms.rogueUnderwearTint = tint
      shader.uniforms.cavemanUnderwearTint = skinTint
      shader.uniforms.cavemanSkinColor = skinColor
      shader.vertexShader =
        `varying float rogueUnderwearHeight;\n${shader.vertexShader}`.replace(
          '#include <begin_vertex>',
          '#include <begin_vertex>\nrogueUnderwearHeight = position.y;'
        )
      shader.fragmentShader =
        `uniform float rogueUnderwearTint;\nuniform float cavemanUnderwearTint;\nuniform vec3 cavemanSkinColor;\nvarying float rogueUnderwearHeight;\n${shader.fragmentShader}`
          .replace(
            '#include <map_fragment>',
            `#include <map_fragment>
          float palest = min(diffuseColor.r, min(diffuseColor.g, diffuseColor.b));
          float brightest = max(diffuseColor.r, max(diffuseColor.g, diffuseColor.b));
          float underwearPrint = rogueUnderwearTint
            * step(0.88, rogueUnderwearHeight) * step(rogueUnderwearHeight, 1.13)
            * smoothstep(0.22, 0.30, palest)
            * smoothstep(0.40, 0.55, palest / max(brightest, 0.001));
          #if defined(USE_COLOR) || defined(USE_COLOR_ALPHA)
            underwearPrint *= step(0.02, vColor.r);
          #endif
          diffuseColor.rgb *= mix(vec3(1.0), vec3(0.008, 0.009, 0.012), underwearPrint);
          float cavemanPrint = cavemanUnderwearTint
            * step(0.88, rogueUnderwearHeight) * step(rogueUnderwearHeight, 1.13)
            * smoothstep(0.24, 0.42, palest)
            * smoothstep(0.52, 0.72, palest / max(brightest, 0.001));
          diffuseColor.rgb = mix(diffuseColor.rgb, cavemanSkinColor, cavemanPrint);
          float underwearFinish = max(underwearPrint, rogueUnderwearTint
            * smoothstep(1.06, 1.075, rogueUnderwearHeight)
            * (1.0 - smoothstep(1.125, 1.14, rogueUnderwearHeight)));
          #if defined(USE_COLOR) || defined(USE_COLOR_ALPHA)
            underwearFinish = max(underwearFinish, rogueUnderwearTint * (1.0 - step(0.02, vColor.r)));
          #endif
        `
          )
          .replace(
            '#include <roughnessmap_fragment>',
            `#include <roughnessmap_fragment>
          roughnessFactor = mix(roughnessFactor, max(roughnessFactor, 0.85), underwearFinish);
          roughnessFactor = mix(roughnessFactor, 0.65, cavemanPrint);
        `
          )
          .replace(
            '#include <metalnessmap_fragment>',
            `#include <metalnessmap_fragment>
          metalnessFactor *= 1.0 - max(underwearFinish, cavemanPrint);
        `
          )
          .replace(
            '#include <lights_physical_fragment>',
            `#include <lights_physical_fragment>
          material.specularColor *= 1.0 - 0.85 * underwearPrint;
        `
          )
    }
    material.needsUpdate = true
  }
  return (black: boolean, skin = false) => {
    tint.value = black ? 1 : 0
    skinTint.value = skin && !black ? 1 : 0
    for (const geometry of new Set([source, mesh.geometry])) {
      if (black || original)
        geometry.setAttribute('color', black ? colors : original)
      else geometry.deleteAttribute('color')
    }
    for (const { material, vertexColors } of materials) {
      const enabled = black || vertexColors
      if (material.vertexColors !== enabled) {
        material.vertexColors = enabled
        material.needsUpdate = true
      }
    }
  }
}
