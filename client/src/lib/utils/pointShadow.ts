import type {
  DepthTexture,
  LightShadow,
  PerspectiveCamera,
  PointLight,
  RenderTarget,
} from 'three'
import {
  Fn,
  If,
  abs,
  add,
  cross,
  cubeTexture,
  float,
  interleavedGradientNoise,
  normalize,
  reference,
  renderGroup,
  screenCoordinate,
  texture,
  uniform,
  vec2,
  vec3,
  viewZToLogarithmicDepth,
  viewZToPerspectiveDepth,
  viewZToReversedPerspectiveDepth,
  vogelDiskSample as vogelDiskSampleTyped,
} from 'three/tsl'
import { Material, PointShadowNode, type NodeBuilder } from 'three/webgpu'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node
const vogelDiskSample: N = vogelDiskSampleTyped

// three@0.186.1 copies every caster's alphaTest onto the one shadow material
// of a light, and the setter bumps `version` on each switch between cut-out
// and opaque casters. Every shadow render object then recomputed its material
// cache key each frame (a quarter of the frame at night). Those objects are
// built with their own caster's state, so the bump tells them nothing.
// Re-check on three upgrades.
const alphaTest = Object.getOwnPropertyDescriptor(
  Material.prototype,
  'alphaTest'
)!
Object.defineProperty(Material.prototype, 'alphaTest', {
  ...alphaTest,
  set(
    this: Material & { _alphaTest: number; isShadowPassMaterial?: boolean },
    value: number
  ) {
    if (this.isShadowPassMaterial) this._alphaTest = value
    else alphaTest.set!.call(this, value)
  },
})

type ShadowTarget = { shadowMap: RenderTarget; depthTexture: DepthTexture }
type TargetFactory = (shadow: LightShadow, builder: NodeBuilder) => ShadowTarget
type FilterInputs = {
  depthTexture: DepthTexture
  shadowCoord: N
  shadow: LightShadow
  depthLayer?: N
}

// three@0.186.1's shadow filters create their render-group uniforms inside
// Fn bodies, i.e. anew on every shader build. Each set gets a shared bind
// group that is cached for good (and the point filter's callbacks also hold
// the builder), so every object that received a shadow stayed in memory.
// These copies (pointShadowFilter, PointShadowFilter, PCFShadowFilter) make
// their uniforms once. Re-check on three upgrades; the game uses PCF only.

/** PCFShadowFilter for `shadow.filterNode`: plain nodes, built once per shadow. */
export function pcfShadowFilter({
  depthTexture,
  shadowCoord,
  shadow,
  depthLayer,
}: FilterInputs): N {
  const mapSize = (reference('mapSize', 'vec2', shadow) as N).setGroup(
    renderGroup
  )
  const radiusScaled = (reference('radius', 'float', shadow) as N)
    .setGroup(renderGroup)
    .mul(vec2(1).div(mapSize).x)
  const phi = interleavedGradientNoise(screenCoordinate.xy).mul(6.28318530718)
  const sample = (i: number) => {
    const uv = shadowCoord.xy.add(vogelDiskSample(i, 5, phi).mul(radiusScaled))
    const depth = texture(depthTexture, uv)
    return (
      depthTexture.isArrayTexture ? depth.depth(depthLayer!) : depth
    ).compare(shadowCoord.z)
  }
  return add(sample(0), sample(1), sample(2), sample(3), sample(4)).mul(1 / 5)
}

function stablePointShadowFilter(shadow: LightShadow) {
  const near = uniform('float')
    .setGroup(renderGroup)
    .onRenderUpdate(() => (shadow.camera as PerspectiveCamera).near)
  const far = uniform('float')
    .setGroup(renderGroup)
    .onRenderUpdate(() => (shadow.camera as PerspectiveCamera).far)
  const bias = (reference('bias', 'float', shadow) as N).setGroup(renderGroup)
  const mapSize = (reference('mapSize', 'vec2', shadow) as N).setGroup(
    renderGroup
  )
  const texelSize = (reference('radius', 'float', shadow) as N)
    .setGroup(renderGroup)
    .div(mapSize.x)
  return Fn(
    ({ depthTexture, shadowCoord }: FilterInputs, builder: NodeBuilder) => {
      // For point lights shadowCoord holds the light-to-fragment vector.
      const position = shadowCoord.xyz.toConst()
      const absPos = position.abs().toConst()
      const viewZ = absPos.x.max(absPos.y).max(absPos.z)
      const result = float(1).toVar()
      If(
        viewZ
          .sub(far)
          .lessThanEqual(0)
          .and(viewZ.sub(near).greaterThanEqual(0)),
        () => {
          const { reversedDepthBuffer, logarithmicDepthBuffer } =
            builder.renderer
          const toDepth = logarithmicDepthBuffer
            ? viewZToLogarithmicDepth
            : viewZToPerspectiveDepth
          const dp = reversedDepthBuffer
            ? viewZToReversedPerspectiveDepth(viewZ.negate(), near, far).sub(
                bias
              )
            : toDepth(viewZ.negate(), near, far).add(bias)
          const bd3D = position.normalize()
          const dir: N = abs(bd3D)
          const up = dir.x
            .greaterThan(dir.z)
            .select(vec3(0, 1, 0), vec3(1, 0, 0))
          const tangent = normalize(cross(bd3D, up))
          const bitangent = cross(bd3D, tangent)
          const phi = interleavedGradientNoise(screenCoordinate.xy).mul(
            6.28318530718
          )
          const sample = (i: number) => {
            const s = vogelDiskSample(i, 5, phi)
            const offset = tangent.mul(s.x).add(bitangent.mul(s.y))
            return cubeTexture(
              depthTexture as N,
              bd3D.add(offset.mul(texelSize))
            ).compare(dp)
          }
          result.assign(
            add(sample(0), sample(1), sample(2), sample(3), sample(4)).mul(
              1 / 5
            )
          )
        }
      )
      return result
    }
  )
}

export function setupPointShadow(light: PointLight): void {
  if (light.shadow.shadowNode) return
  const node = new PointShadowNode(light, light.shadow) as PointShadowNode & {
    setupRenderTarget: TargetFactory
    setupShadowFilter: (builder: NodeBuilder, inputs: FilterInputs) => N
  }
  const createTarget = node.setupRenderTarget.bind(node)
  node.setupRenderTarget = (shadow, builder) => {
    const target = createTarget(shadow, builder)
    // Allocate before compileAsync binds it; the first shadow pass must not recreate it.
    builder.renderer.initRenderTarget(target.shadowMap)
    return target
  }
  const filter = stablePointShadowFilter(light.shadow)
  node.setupShadowFilter = (_builder, inputs) => filter(inputs)
  light.shadow.shadowNode = node
}
