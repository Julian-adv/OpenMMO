import * as THREE from 'three'
import {
  MeshBasicNodeMaterial,
  MeshStandardNodeMaterial,
  SpriteNodeMaterial,
} from 'three/webgpu'
import {
  attribute,
  cameraPosition,
  cos,
  exp,
  float,
  fract,
  length,
  max,
  mix,
  modelWorldMatrixInverse,
  normalView,
  normalWorld,
  positionLocal,
  sin,
  smoothstep,
  texture,
  time,
  uniform,
  uv,
  vertexColor,
  vec3,
  vec4,
} from 'three/tsl'
import spots from '../data/winterTreeLights.json'
import { serverGameTime } from '../stores/timeStore'
import { OBJECT_SNOW, perlin, snowCover } from '../shaders/snow-cover-nodes'

type Point = [number, number, number]
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any

/** Bulb centres and the star of `winter_tree.glb`, model-local, from
 *  tools/prepare_winter_tree.py. */
export const WINTER_TREE_LIGHTS = spots.bulbs as Point[]
export const WINTER_TREE_STAR = spots.star as Point
/** The model against the 5 m tree the sizes below were tuned on. */
const SIZE = spots.scale
/** Height above the tree's base of the warm light it throws at night. */
export const WINTER_TREE_LIGHT_HEIGHT = 2.4 * SIZE

const BULB_COLORS = ['#ffc46b', '#ff5d4a', '#ffc46b', '#58d37a', '#5aa2ff']
const BULB_RADIUS = 0.13 * SIZE
const BULB_BRIGHTNESS = 1.3
const STAR_COLOR = '#ffd27a'
const STAR_HALO_RADIUS = 0.42 * SIZE
const STAR_FLARE_SIZE = 1.15 * SIZE
const GLINT_COUNT = 18
const GLINT_SIZE = 0.42 * SIZE
const MOTE_COUNT = 28
const MOTE_COLOR = '#ffd9a0'
const DAY_GLOW = 0.15
/** Daylight shows the boughs pale and thin, with the core plain between
 *  them. By day they are pulled to a deep fir green and cut out fuller, and
 *  the core is shaded. Night keeps the model as built. */
const DAY_FOLIAGE_TINT = [0.5, 0.72, 1.25]
const DAY_FOLIAGE_CUTOFF = 0.2
const DAY_CORE_SHADE = 0.5
/** Share of the lying snow the boughs hold. */
const BOUGH_SNOW = 0.55
/** The solid cone inside the boughs in `winter_tree.glb`. */
const CORE = 'winter_tree_core'
/** Below this are the skirt and the presents, which stay unlit. */
const DECOR_FLOOR = 0.75 * SIZE
const GOLDEN_ANGLE = 2.399963229728653
const TAU = Math.PI * 2

const crownTop = WINTER_TREE_STAR[1] - 0.3 * SIZE

let dark: ReturnType<typeof createNight> | null = null

function createNight() {
  const night = uniform(0)
  serverGameTime.subscribe((now) => {
    night.value = now?.isNight ? 1 : 0
  })
  return night
}

/** 1 at night and 0 by day, shared by every tree. */
const night = () => (dark ??= createNight())
/** 1 at night and faint by day. */
const nightGlow = (): N => mix(DAY_GLOW, 1, night())

function additive<T extends THREE.Material>(material: T): T {
  material.transparent = true
  material.depthWrite = false
  material.blending = THREE.AdditiveBlending
  material.toneMapped = false
  return material
}

/** A glow that is brightest where its sphere faces the camera, gone at the rim. */
function haloMaterial(strength: N, falloff: number) {
  const material = additive(new MeshBasicNodeMaterial())
  material.opacityNode = normalView.z.abs().pow(falloff).mul(strength)
  return material
}

/** `point` moved `distance` towards the camera, clear of the mesh it sits on. */
function towardCamera(point: N, distance: number) {
  const eye = modelWorldMatrixInverse.mul(vec4(cameraPosition, 1)).xyz
  return point.add(eye.sub(point).normalize().mul(distance))
}

/** A soft core crossed by two thin rays, over a sprite quad. */
function glintShape(sharpness: number) {
  const d = uv().sub(0.5)
  const reach = length(d).mul(2).oneMinus().clamp().pow(2)
  const rays = float(1).div(d.x.abs().mul(d.y.abs()).mul(sharpness).add(1))
  return reach.mul(rays).add(exp(length(d).pow(2).mul(-70)))
}

function instanced(
  geometry: THREE.BufferGeometry,
  name: string,
  size: number,
  data: number[]
) {
  geometry.setAttribute(
    name,
    new THREE.InstancedBufferAttribute(new Float32Array(data), size)
  )
}

/** Bulb halos: a band of light climbs the tree in a spiral over a faint flicker. */
function buildBulbs() {
  const count = WINTER_TREE_LIGHTS.length
  const geometry = new THREE.SphereGeometry(BULB_RADIUS, 8, 5)
  instanced(
    geometry,
    'aBand',
    1,
    WINTER_TREE_LIGHTS.map(([x, y, z]) => (y / SIZE) * 1.4 + Math.atan2(z, x))
  )
  instanced(
    geometry,
    'aPhase',
    1,
    WINTER_TREE_LIGHTS.map((_, i) => (i * GOLDEN_ANGLE) % TAU)
  )
  const band = sin(time.mul(0.9).sub(attribute('aBand', 'float')))
    .mul(0.5)
    .add(0.5)
  const flicker = sin(time.mul(3.1).add(attribute('aPhase', 'float')))
    .mul(0.5)
    .add(0.5)
  const strength = band.pow(2).mul(0.6).add(flicker.mul(0.12)).add(0.28)

  const bulbs = new THREE.InstancedMesh(
    geometry,
    haloMaterial(strength.mul(nightGlow()), 4),
    count
  )
  bulbs.name = 'winter-tree-bulbs'
  const matrix = new THREE.Matrix4()
  const tint = new THREE.Color()
  WINTER_TREE_LIGHTS.forEach(([x, y, z], i) => {
    bulbs.setMatrixAt(i, matrix.makeTranslation(x, y, z))
    bulbs.setColorAt(
      i,
      tint
        .set(BULB_COLORS[i % BULB_COLORS.length])
        .multiplyScalar(BULB_BRIGHTNESS)
    )
  })
  return bulbs
}

/** The star's breathing halo and its slowly rocking flare. */
function buildStar() {
  const star = new THREE.Group()
  star.name = 'winter-tree-star'
  star.position.fromArray(WINTER_TREE_STAR)
  const breath = sin(time.mul(1.1)).mul(0.12).add(0.88)

  const halo = new THREE.Mesh(
    new THREE.SphereGeometry(STAR_HALO_RADIUS, 16, 10),
    haloMaterial(breath.mul(0.35).mul(nightGlow()), 5)
  )
  ;(halo.material as MeshBasicNodeMaterial).color.set(STAR_COLOR)

  const material = additive(new SpriteNodeMaterial())
  material.color.set(STAR_COLOR)
  material.positionNode = towardCamera(vec3(0), 0.4 * SIZE)
  material.rotationNode = sin(time.mul(0.27)).mul(0.4)
  material.opacityNode = glintShape(900)
    .mul(sin(time.mul(1.7)).mul(0.1).add(0.45))
    .mul(nightGlow())
  const flare = new THREE.Mesh(
    new THREE.PlaneGeometry(STAR_FLARE_SIZE, STAR_FLARE_SIZE),
    material
  )
  star.add(halo, flare)
  return star
}

/** A sprite per instance; `aSpot`-style attributes place them in the shader, so
 *  the bounds are those of the whole tree. */
function spriteField(
  count: number,
  material: SpriteNodeMaterial,
  name: string
) {
  const field = new THREE.InstancedMesh(
    new THREE.PlaneGeometry(1, 1),
    material,
    count
  )
  field.name = name
  field.boundingSphere = new THREE.Sphere(
    new THREE.Vector3(0, crownTop / 2, 0),
    crownTop
  )
  return field
}

/** Brief four-point sparkles on the boughs, each a little off a bulb. */
function buildGlints() {
  const material = additive(new SpriteNodeMaterial())
  material.color.set('#fff4d6')
  const field = spriteField(GLINT_COUNT, material, 'winter-tree-glints')
  const spots: number[] = []
  const beat: number[] = []
  const stride = Math.floor(WINTER_TREE_LIGHTS.length / GLINT_COUNT)
  const turn = new THREE.Vector3()
  for (let i = 0; i < GLINT_COUNT; i++) {
    turn
      .fromArray(WINTER_TREE_LIGHTS[i * stride])
      .applyAxisAngle(THREE.Object3D.DEFAULT_UP, 0.4)
    spots.push(turn.x, turn.y + 0.15 * SIZE, turn.z)
    beat.push(0.6 + ((i * 5) % 9) * 0.1, (i * 1.7) % TAU)
  }
  instanced(field.geometry, 'aSpot', 3, spots)
  instanced(field.geometry, 'aBeat', 2, beat)

  const pace = attribute('aBeat', 'vec2')
  const flash = max(sin(time.mul(pace.x).add(pace.y)), 0).pow(24)
  material.positionNode = towardCamera(attribute('aSpot', 'vec3'), 0.12 * SIZE)
  material.scaleNode = flash.mul(0.5).add(0.5).mul(GLINT_SIZE)
  material.rotationNode = pace.y
  material.opacityNode = glintShape(1400)
    .mul(flash)
    .mul(nightGlow().mul(0.5).add(0.5))
  return field
}

/** Gold dust sinking slowly around the tree. */
function buildMotes() {
  const material = additive(new SpriteNodeMaterial())
  material.color.set(MOTE_COLOR)
  const field = spriteField(MOTE_COUNT, material, 'winter-tree-motes')
  const orbit: number[] = []
  for (let i = 0; i < MOTE_COUNT; i++)
    orbit.push(
      i * GOLDEN_ANGLE,
      (0.7 + ((i * 0.618034) % 1) * 1.3) * SIZE,
      0.035 + ((i * 3) % 7) * 0.006,
      (i * 0.381966) % 1
    )
  instanced(field.geometry, 'aOrbit', 4, orbit)

  const seed = attribute('aOrbit', 'vec4')
  const fall = fract(time.mul(seed.z).add(seed.w))
  const angle = seed.x.add(time.mul(0.12))
  const radius = seed.y.add(
    sin(time.mul(0.6).add(seed.w.mul(TAU))).mul(0.12 * SIZE)
  )
  material.positionNode = vec3(
    cos(angle).mul(radius),
    fall
      .oneMinus()
      .mul(crownTop + 0.4 * SIZE)
      .add(0.3 * SIZE),
    sin(angle).mul(radius)
  )
  material.scaleNode = seed.w.mul(0.05).add(0.08).mul(SIZE)
  material.opacityNode = exp(length(uv().sub(0.5)).pow(2).mul(-22))
    .mul(sin(fall.mul(Math.PI)))
    .mul(sin(time.mul(2.3).add(seed.x)).mul(0.3).add(0.7))
    .mul(nightGlow())
  return field
}

/** Everything that glows over the winter tree model. */
export function buildWinterTreeGlow(): THREE.Group {
  const group = new THREE.Group()
  group.name = 'winter-tree-glow'
  group.add(buildBulbs(), buildStar(), buildGlints(), buildMotes())
  return group
}

/** The boughs as built at night, deeper and fuller by day, dusted with
 *  whatever snow lies around. */
function boughMaterial(src: THREE.MeshStandardMaterial) {
  const material = new MeshStandardNodeMaterial({
    name: src.name,
    normalMap: src.normalMap,
    roughnessMap: src.roughnessMap,
    metalnessMap: src.metalnessMap,
    roughness: src.roughness,
    metalness: src.metalness,
    side: src.side,
  })
  const day = night().oneMinus()
  const leaf = texture(src.map!)
  const tint = mix(vec3(1), vec3(...DAY_FOLIAGE_TINT), day)
  // Even over the tree: the ground's snow patches are metres wide and would
  // whiten one side of it.
  const grain = perlin(
    positionLocal.xz.add(positionLocal.y.mul(0.6)),
    2.2,
    0.37,
    11.3
  )
  const snow = snowCover
    .mul(smoothstep(0.25, 0.7, normalWorld.y))
    .mul(smoothstep(0.3, 0.6, grain).mul(0.65).add(0.35))
    .mul(BOUGH_SNOW)
    .clamp()
  material.colorNode = vec4(mix(leaf.rgb.mul(tint), OBJECT_SNOW, snow), leaf.a)
  material.roughnessNode = mix(float(src.roughness), float(0.85), snow)
  material.alphaTestNode = mix(src.alphaTest, DAY_FOLIAGE_CUTOFF, day)
  return material
}

/** The dark core behind the boughs, sunk further into shadow by day. */
function coreMaterial(src: THREE.MeshStandardMaterial) {
  const material = new MeshStandardNodeMaterial({
    name: src.name,
    roughness: src.roughness,
    metalness: src.metalness,
    side: src.side,
  })
  const shade = mix(1, DAY_CORE_SHADE, night().oneMinus())
  material.colorNode = vec3(...src.color.toArray()).mul(shade)
  return material
}

/** Lets the star and the ornaments keep their colour in the dark. */
function lightDecorations(model: THREE.Object3D) {
  const height = positionLocal.y
  const star = smoothstep(crownTop - 0.15 * SIZE, crownTop + 0.1 * SIZE, height)
  const shine = smoothstep(DECOR_FLOOR, DECOR_FLOOR + 0.3 * SIZE, height)
    .mul(star.mul(0.35).add(0.3))
    .mul(nightGlow())
  model.traverse((o) => {
    if (!(o instanceof THREE.Mesh)) return
    const src = o.material as THREE.MeshStandardMaterial
    // The cut-outs are the boughs and the sprigs wound round them.
    if (src.alphaTest > 0 && src.map) {
      o.material = boughMaterial(src)
      return
    }
    if (src.name === CORE) {
      o.material = coreMaterial(src)
      return
    }
    // Textured ornaments and the painted baubles.
    if (src.alphaTest > 0 || !(src.map || src.vertexColors)) return
    o.geometry.computeBoundingBox()
    if (o.geometry.boundingBox!.max.y < DECOR_FLOOR) return
    const material = new MeshStandardNodeMaterial({
      name: src.name,
      map: src.map,
      normalMap: src.normalMap,
      roughnessMap: src.roughnessMap,
      metalnessMap: src.metalnessMap,
      roughness: src.roughness,
      metalness: src.metalness,
      side: src.side,
      vertexColors: src.vertexColors,
    })
    const tint = src.map ? texture(src.map).rgb : vertexColor().rgb
    material.emissiveNode = tint.mul(shine)
    o.material = material
  })
}

/** Dresses a freshly loaded winter tree model: lit decorations and its glow. */
export function decorateWinterTree(model: THREE.Object3D) {
  lightDecorations(model)
  model.add(buildWinterTreeGlow())
}
