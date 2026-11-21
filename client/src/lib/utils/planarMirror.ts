import * as THREE from 'three'
import { MeshStandardNodeMaterial, type WebGPURenderer } from 'three/webgpu'
import {
  mix,
  positionWorld,
  texture,
  uniform,
  vec2,
  vec3,
  vec4,
} from 'three/tsl'
import { renderKeepingLights } from './deferredShaderBuild'

/** Mirrors farther than this from the viewer's character stay a plain plate. */
const ACTIVE_RANGE = 9
const PLATE = new THREE.Color(0.74, 0.84, 0.9)
/** Target side: at most, and rounded up to. */
const MAX_SIDE = 512
const STEP = 32
/** NDC margin around the mirrors so filtering never reads the edge. */
const MARGIN = 0.02
/** Downward tilt that puts a seated head 1.1 m out mid-plate. */
const TILT = THREE.MathUtils.degToRad(30)
/** Metres shown in front of the wall, and how far behind it the view starts. */
const REACH = 7
const BEHIND = 10

const target = new THREE.RenderTarget(STEP, STEP, { type: THREE.HalfFloatType })
const viewProjection = uniform(new THREE.Matrix4())
const active = uniform(0)

const clip = viewProjection.mul(vec4(positionWorld, 1))
const ndc = clip.xy.div(clip.w)
const reflection = texture(
  target.texture,
  vec2(ndc.x.mul(0.5).add(0.5), ndc.y.mul(-0.5).add(0.5))
)

/** Every wall mirror's plate: lit and pale, or the reflection while one is drawn. */
export const mirrorPlateMaterial = new MeshStandardNodeMaterial({
  roughness: 0.08,
  metalness: 0,
})
mirrorPlateMaterial.name = 'Mirror plate'
mirrorPlateMaterial.colorNode = mix(
  vec3(PLATE.r, PLATE.g, PLATE.b),
  vec3(0),
  active
)
mirrorPlateMaterial.emissiveNode = reflection.rgb.mul(0.92).mul(active)

const _normal = new THREE.Vector3()
const _point = new THREE.Vector3()
const _eye = new THREE.Vector3()
const _origin = new THREE.Vector3()
const _axis = new THREE.Vector3()
const _look = new THREE.Vector3()
const _plane = new THREE.Plane()
const _clip = new THREE.Vector4()
const _far = new THREE.Vector4()
const _corner = new THREE.Vector3()
const _size = new THREE.Vector2()
const _box = new THREE.Box3()
const _crop = new THREE.Matrix4()
const _inverse = new THREE.Matrix4()

let virtual: THREE.OrthographicCamera | null = null
let shadowLights: THREE.LightShadow[] = []
let outdoors: THREE.Object3D[] = []
let shadowsFoundAt = -Infinity

/** Mirrors that share the wall of the one nearest `viewer`, if that one is in range and faces it. */
export function mirrorsToReflect(
  mirrors: THREE.Mesh[],
  viewer: THREE.Vector3
): THREE.Mesh[] {
  let nearest: THREE.Mesh | null = null
  let best = ACTIVE_RANGE * ACTIVE_RANGE
  for (const mirror of mirrors) {
    const distance = _point
      .setFromMatrixPosition(mirror.matrixWorld)
      .distanceToSquared(viewer)
    if (distance < best) {
      best = distance
      nearest = mirror
    }
  }
  if (!nearest) return []
  _axis.set(0, 0, 1).transformDirection(nearest.matrixWorld)
  _origin.setFromMatrixPosition(nearest.matrixWorld)
  if (_point.copy(viewer).sub(_origin).dot(_axis) <= 0) return []
  return mirrors.filter(
    (mirror) =>
      _normal.set(0, 0, 1).transformDirection(mirror.matrixWorld).dot(_axis) >
        0.999 &&
      Math.abs(
        _point.setFromMatrixPosition(mirror.matrixWorld).sub(_origin).dot(_axis)
      ) < 0.02
  )
}

export function stopMirrorReflection() {
  active.value = 0
}

/** Draws what `mirrors` (on one plane) show: cropped to their outline and cut at the wall. */
export function renderMirrorReflection(
  renderer: WebGPURenderer,
  scene: THREE.Scene,
  mirrors: THREE.Mesh[]
) {
  const first = mirrors[0]
  const normal = _normal.set(0, 0, 1).transformDirection(first.matrixWorld)
  const point = _point.setFromMatrixPosition(first.matrixWorld)

  // Not the true reflection: a fixed view straight out from the wall, tilted down onto a seated face.
  virtual ??= new THREE.OrthographicCamera(
    -REACH,
    REACH,
    REACH,
    -REACH,
    0.1,
    BEHIND + REACH
  )
  virtual.coordinateSystem = renderer.coordinateSystem
  virtual.updateProjectionMatrix()
  _look
    .copy(normal)
    .multiplyScalar(Math.cos(TILT))
    .setY(-Math.sin(TILT))
    .normalize()
  virtual.position.copy(point).addScaledVector(_look, -BEHIND)
  virtual.up.set(0, 1, 0)
  virtual.lookAt(_eye.copy(point).add(_look))
  virtual.updateMatrixWorld()
  virtual.matrixWorldInverse.copy(virtual.matrixWorld).invert()

  // Crop the view to the mirrors' outline as this camera sees it.
  const projection = virtual.projectionMatrix
  _box.makeEmpty()
  for (const mirror of mirrors) {
    const bounds = (mirror.geometry.boundingBox ??
      (mirror.geometry.computeBoundingBox(), mirror.geometry.boundingBox))!
    for (let i = 0; i < 8; i++) {
      _corner
        .set(
          i & 1 ? bounds.max.x : bounds.min.x,
          i & 2 ? bounds.max.y : bounds.min.y,
          i & 4 ? bounds.max.z : bounds.min.z
        )
        .applyMatrix4(mirror.matrixWorld)
        .applyMatrix4(virtual.matrixWorldInverse)
        .applyMatrix4(projection)
      _box.expandByPoint(_corner)
    }
  }
  const x0 = Math.max(-1, _box.min.x - MARGIN)
  const x1 = Math.min(1, _box.max.x + MARGIN)
  const y0 = Math.max(-1, _box.min.y - MARGIN)
  const y1 = Math.min(1, _box.max.y + MARGIN)
  if (x1 <= x0 || y1 <= y0) {
    active.value = 0
    return
  }
  _crop
    .makeScale(2 / (x1 - x0), 2 / (y1 - y0), 1)
    .setPosition(-(x1 + x0) / (x1 - x0), -(y1 + y0) / (y1 - y0), 0)
  projection.premultiply(_crop)

  // Move the near plane onto the wall (Lengyel's oblique clipping, for any projection).
  _plane
    .setFromNormalAndCoplanarPoint(normal, point)
    .applyMatrix4(virtual.matrixWorldInverse)
  _clip.set(_plane.normal.x, _plane.normal.y, _plane.normal.z, _plane.constant)
  _inverse.copy(projection).invert()
  _far.set(Math.sign(_clip.x), Math.sign(_clip.y), 1, 1).applyMatrix4(_inverse)
  const webgpu = renderer.coordinateSystem === THREE.WebGPUCoordinateSystem
  _clip.multiplyScalar((webgpu ? 1 : 2) / _clip.dot(_far))
  const e = projection.elements
  e[2] = _clip.x - (webgpu ? 0 : e[3])
  e[6] = _clip.y - (webgpu ? 0 : e[7])
  e[10] = _clip.z - (webgpu ? 0 : e[11])
  e[14] = _clip.w - (webgpu ? 0 : e[15])
  virtual.projectionMatrixInverse.copy(projection).invert()

  renderer.getDrawingBufferSize(_size)
  const side = (pixels: number) =>
    Math.min(MAX_SIDE, Math.max(STEP, Math.ceil(pixels / STEP) * STEP))
  const width = side(((x1 - x0) / 2) * _size.x)
  const height = side(((y1 - y0) / 2) * _size.y)
  if (target.width !== width || target.height !== height)
    target.setSize(width, height)

  // A second camera would redraw the lights' shadow maps.
  const now = performance.now()
  if (now - shadowsFoundAt > 2000) {
    shadowsFoundAt = now
    shadowLights = []
    outdoors = []
    scene.traverse((object) => {
      const shadow = (object as { shadow?: THREE.LightShadow }).shadow
      if (shadow) shadowLights.push(shadow)
      // One instanced mesh spans the whole view, so culling cannot drop it.
      if (
        (object as THREE.InstancedMesh).isInstancedMesh ||
        object.name === 'water'
      )
        outdoors.push(object)
    })
  }
  const shadows = shadowLights.map(
    (shadow) => [shadow, shadow.autoUpdate, shadow.needsUpdate] as const
  )
  for (const shadow of shadowLights) {
    shadow.autoUpdate = false
    shadow.needsUpdate = false
  }
  const hidden = outdoors.filter((object) => object.visible)
  for (const object of hidden) object.visible = false
  const previousTarget = renderer.getRenderTarget()
  const previousMRT = renderer.getMRT()
  const previousAutoClear = renderer.autoClear
  mirrorPlateMaterial.visible = false
  renderer.setMRT(null)
  renderer.setRenderTarget(target)
  renderer.autoClear = true
  renderKeepingLights(renderer, scene, virtual)
  renderer.setMRT(previousMRT)
  renderer.setRenderTarget(previousTarget)
  renderer.autoClear = previousAutoClear
  mirrorPlateMaterial.visible = true
  for (const object of hidden) object.visible = true
  for (const [shadow, autoUpdate, needsUpdate] of shadows) {
    shadow.autoUpdate = autoUpdate
    shadow.needsUpdate = needsUpdate
  }

  viewProjection.value.multiplyMatrices(projection, virtual.matrixWorldInverse)
  active.value = 1
}
