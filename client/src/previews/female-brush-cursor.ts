import * as THREE from 'three'

/** Draws the skin brush's reach on the body: rest-space distance, as the brush picks triangles. */
export function femaleBrushCursor(mesh: THREE.Mesh) {
  const center = { value: new THREE.Vector3() }
  const radius = { value: 0 }
  const visible = { value: 0 }
  const color = { value: new THREE.Color() }
  const materials = (
    Array.isArray(mesh.material) ? mesh.material : [mesh.material]
  ).filter((material) => material instanceof THREE.MeshStandardMaterial)
  for (const material of materials) {
    const compile = material.onBeforeCompile
    const cacheKey = material.customProgramCacheKey()
    material.customProgramCacheKey = () => `${cacheKey}:brush-cursor`
    material.onBeforeCompile = (shader, renderer) => {
      compile.call(material, shader, renderer)
      shader.uniforms.brushCenter = center
      shader.uniforms.brushRadius = radius
      shader.uniforms.brushVisible = visible
      shader.uniforms.brushColor = color
      shader.vertexShader =
        `varying vec3 brushRest;\n${shader.vertexShader}`.replace(
          '#include <begin_vertex>',
          '#include <begin_vertex>\nbrushRest = position;'
        )
      shader.fragmentShader = `varying vec3 brushRest;
uniform vec3 brushCenter;
uniform float brushRadius;
uniform float brushVisible;
uniform vec3 brushColor;
${shader.fragmentShader}`.replace(
        '#include <dithering_fragment>',
        `#include <dithering_fragment>
float brushDistance = distance(brushRest, brushCenter);
float brushEdge = abs(brushDistance - brushRadius);
float brushWidth = max(fwidth(brushDistance) * 1.5, 0.0008);
float brushRing = 1.0 - smoothstep(brushWidth * 0.5, brushWidth, brushEdge);
float brushFill = 0.15 * step(brushDistance, brushRadius);
gl_FragColor.rgb = mix(gl_FragColor.rgb, brushColor, brushVisible * max(brushRing, brushFill));`
      )
    }
    material.needsUpdate = true
  }
  return {
    show(point: THREE.Vector3, size: number, tint: THREE.ColorRepresentation) {
      center.value.copy(point)
      radius.value = size
      color.value.set(tint)
      visible.value = 1
    },
    hide() {
      visible.value = 0
    },
  }
}
