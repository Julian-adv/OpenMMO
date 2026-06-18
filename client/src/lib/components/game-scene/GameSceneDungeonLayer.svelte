<script lang="ts">
  /**
   * GameSceneDungeonLayer — renders the dungeon floor the local player is
   * on. Geometry comes from the shared wasm layout (see dungeonManager);
   * only the current depth is built, rebuilt on depth/dungeon change.
   * Stair shafts are part of both adjacent floors' groups with identical
   * world-space geometry, so the midpoint floor switch is seamless.
   */
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import { SvelteMap } from 'svelte/reactivity'
  import {
    currentDungeonDepth,
    currentDungeonId,
    dungeonDoorOpen,
    dungeonPropsResetRevision,
    dungeonPropsRevision,
  } from '../../stores/dungeonStore'
  import {
    dungeonManager,
    type DungeonFloorLayout,
  } from '../../managers/dungeonManager'
  import { objectManager } from '../../managers/objectManager'
  import { loadGLB } from '../../utils/gltfCache'
  import { rotatedRectAabb } from '../../utils/objectFootprint'
  import { createRng } from '../../utils/simplex-noise'
  import type { DoorLeaf } from '../../utils/dungeon-geometry'
  import { networkManager } from '../../network/socket'
  import {
    buildDungeonEntranceGroup,
    buildDungeonFloorGroup,
    disposeDungeonGroup,
    UP_SHAFT_GROUP_NAME,
    type WallRun,
  } from '../../utils/dungeon-geometry'
  import { getGhostHousingMaterial } from '../../utils/housing-textures'
  import { isoCameraOccludesPlayer } from '../../utils/iso-occlusion'
  import { passabilityDebugVisible } from '../../stores/debugStore'
  import { pushPassabilityEdges } from '../../utils/passability-wireframe'

  interface Props {
    /** Fired the frame the player comes into range of a clicked barrel/crate,
     *  handing off to the player swing that breaks it at the contact frame. */
    onPropReady?: (
      entranceId: string,
      depth: number,
      propId: number,
      x: number,
      z: number
    ) => void
  }
  let { onPropReady }: Props = $props()

  /** Walk-up-to-open range for the treasure chest (matches the server). */
  const CHEST_OPEN_RANGE = 1.8
  let chestRequested = false

  /** Once the player walking up to a clicked prop is within this range, the
   *  break/open is requested. Kept inside the server's 2.5m so a borderline
   *  float position never gets rejected. */
  const PROP_INTERACT_TRIGGER_RANGE = 2.0
  /** Catalog id of the broken variant rendered when a prop is destroyed. */
  const BROKEN_VARIANT: Record<string, string> = {
    barrel: 'barrel_broken',
    crate: 'crate_pieces',
  }
  const isBreakable = (kind: string) => kind in BROKEN_VARIANT
  /** Dungeon chest props render from this catalog id (the animated GLB, not the
   *  static `chest.glb`) so a click can play the lid-open clip. */
  const CHEST_ANIMATED_ID = 'chest_animated'
  const CHEST_OPEN_CLIP = 'ChestOpen'
  const COIN_SPILL_MODEL_PATH = '/models/objects/coin_pile_spill.glb'
  const COIN_SPILL_FAN_DEG = 70
  const COIN_SPILL_MIN_DIST = 0.55
  const COIN_SPILL_MAX_DIST = 0.9
  /** How far the open lid's rear corner reaches behind the chest origin (along
   *  local −Z): the lid swings up and ~105° back over the hinge. Measured from
   *  chest_animated.glb. The chest is pushed this far (less the half-cell it
   *  already sits in) off its back wall so the opening lid never clips it. */
  const CHEST_LID_BACK_REACH = 0.8
  /** Gap kept between the fully-open lid and the back wall. */
  const CHEST_LID_WALL_GAP = 0.06
  /** Yaw (deg) that maps the model's back (local −Z) onto the chosen back wall. */
  const CHEST_BACK_WALL_YAW = { N: 0, S: 180, W: 90, E: 270 } as const

  /** One placed prop, tracked so a live break can swap just it (not rebuild the
   *  whole floor). `cellX/cellZ` locate the broken debris at the cell center. */
  interface PropEntry {
    clones: THREE.Object3D[]
    kind: string
    propId: number
    cellX: number
    cellZ: number
    rotationDeg: number
    broken: boolean
    /** Chest only: whether its lid-open animation has been started. */
    opened?: boolean
    /** Chest only: the coin spill spawned when the chest opens. */
    coinSpill?: THREE.Object3D
  }
  let propEntries = new Map<number, PropEntry>()
  /** The chest lid-open clip, captured once from the animated chest GLB and
   *  shared by every chest clone (each gets its own AnimationMixer). */
  let chestOpenClip: THREE.AnimationClip | null = null
  /** One-shot prop mixers (chest lid-open + coin-spill clips), ticked each
   *  frame. Each clip is LoopOnce + clampWhenFinished, so it holds its final
   *  pose once done. Cleared with the props on a floor rebuild. */
  let propMixers: THREE.AnimationMixer[] = []

  const root = new THREE.Group()
  let currentGroup: THREE.Group | null = null
  let entranceGroup: THREE.Group | null = null
  /** Decorative room clutter (barrel/crate/chest GLBs) for the current floor,
   *  kept in its own group on `root` — never inside currentGroup, whose
   *  disposer would otherwise dispose these shared cached GLB resources.
   *  Stable reference (created once, children swapped per floor) so the array
   *  handed to click raycasting via getPropMeshes never goes stale. */
  const propsGroup = new THREE.Group()
  root.add(propsGroup)
  /** Per-kind metrics measured once per GLB: base-seat offset (−bbox.min.y),
   *  height, and horizontal half-extents (hx/hz) — used to seat the model, to
   *  align a chest's long side to its wall, and to inset a boxy prop off it. */
  const propMetrics = new SvelteMap<
    string,
    { seatY: number; height: number; hx: number; hz: number }
  >()
  /** Nest a stacked prop slightly into the one below to hide the seam. */
  const PROP_STACK_NEST = 0.97
  /** A crate is left crooked when its random yaw lands more than this many
   *  degrees off a right angle (≈ a quarter of crates). */
  const CRATE_ASKEW_THRESH = 34
  /** Scales that off-axis amount down to a believable lean (~19–25°). */
  const CRATE_ASKEW_SCALE = 0.55
  /** Double entrance doors; swing open/shut when clicked (house-door style). */
  let entranceDoors: DoorLeaf[] = []
  /** Eased open fraction (0 shut → 1 fully open), lerped per frame toward the
   *  click-toggled dungeonDoorOpen store. */
  let doorOpenAmount = 0
  /** Whether the surface entrance is currently shown (depth 0). Tracked so we
   *  can reconcile the door visual to the store on the hidden→shown edge. */
  let entranceVisible = false
  let builtKey = ''
  let entranceKey = ''
  let lastPropsResetRevision = 0

  // ── Up-shaft occlusion fade ──────────────────────────────
  // The staircase you arrive by occludes the player from the iso camera when
  // they walk behind it; fade it to a ghost material (like trees/houses).
  interface GhostMesh {
    mesh: THREE.Mesh
    base: THREE.Material
    ghost: THREE.Material
  }
  let upShaftMeshes: GhostMesh[] = []
  let upShaftAABB: THREE.Box3 | null = null
  let upShaftOccluded = false
  /** Ray inside the AABB before it counts as occluding (matches housing). */
  const MIN_OCCLUSION_DEPTH = 0.3

  // ── Wall-run occlusion fade ──────────────────────────────
  // Any wall run (all four sides) that ends up between the iso camera and the
  // player is ghosted, per-run (per-run AABB, so the others stay solid). The
  // runs are thin (0.1m), so the SW camera ray only ever crosses ~0.1 of one — a
  // much smaller occlusion depth than the bulky up-shaft AABB.
  interface WallRunFade {
    mesh: THREE.Mesh
    base: THREE.Material
    ghost: THREE.Material
    aabb: THREE.Box3
  }
  let wallRuns: WallRunFade[] = []
  const WALL_RUN_MIN_OCCLUSION = 0.05

  function clearGroup() {
    if (currentGroup) {
      root.remove(currentGroup)
      disposeDungeonGroup(currentGroup)
      currentGroup = null
    }
    clearProps()
    upShaftMeshes = []
    upShaftAABB = null
    upShaftOccluded = false
    wallRuns = []
  }

  /** Detach the current floor's props. Their GLB geometry/materials are shared
   *  via the module-level gltfCache, so (unlike disposeDungeonGroup) we only
   *  drop the clones and let GC reclaim them — disposing would corrupt the
   *  cache and every other instance. */
  function clearProps() {
    for (const c of [...propsGroup.children]) propsGroup.remove(c)
    propEntries = new Map()
    for (const m of propMixers) m.stopAllAction()
    propMixers = []
  }

  /** Measure (and cache) a model's seat offset, height and half-extents. */
  function measureProp(kind: string, template: THREE.Object3D) {
    let m = propMetrics.get(kind)
    if (!m) {
      template.updateMatrixWorld(true)
      const box = new THREE.Box3().setFromObject(template)
      const size = box.getSize(new THREE.Vector3())
      m = {
        seatY: -box.min.y,
        height: Math.max(0.1, size.y),
        hx: size.x / 2,
        hz: size.z / 2,
      }
      propMetrics.set(kind, m)
    }
    return m
  }

  /** Strip shadows on, raycast off for a decorative (non-interactive) clone. */
  function tagDecorative(clone: THREE.Object3D) {
    clone.traverse((o) => {
      if (o instanceof THREE.Mesh) {
        o.castShadow = true
        o.receiveShadow = true
        o.raycast = () => {} // never intercept click-to-move
      }
    })
  }

  /**
   * Build one normal (un-broken) prop into `group`/`entries`. Barrels and crates
   * are tagged interactive (clickable to break); chests stay decorative. The
   * per-kind yaw + cell-inset tuning is unchanged from the original placement.
   */
  async function addNormalProp(
    group: THREE.Group,
    entries: Map<number, PropEntry>,
    index: number,
    prop: DungeonFloorLayout['props'][number],
    key: string,
    depth: number,
    carvedAt: (x: number, z: number) => boolean
  ) {
    // Chests render from the animated GLB (lid rigged for the open clip); the
    // other props use their own catalog model.
    const def =
      prop.kind === 'chest'
        ? objectManager.getCatalogEntry(CHEST_ANIMATED_ID)
        : objectManager.getCatalogEntry(prop.kind)
    if (!def) return
    let template: THREE.Object3D
    try {
      const gltf = await loadGLB(`/models/objects/${def.model}`)
      template = gltf.scene
      if (prop.kind === 'chest' && !chestOpenClip && gltf.animations.length) {
        chestOpenClip =
          gltf.animations.find((a) => a.name === CHEST_OPEN_CLIP) ??
          gltf.animations[0]
      }
    } catch {
      return
    }
    if (key !== builtKey) return // floor changed mid-load — abandon
    const m = measureProp(prop.kind, template)

    // Chests sit with their hinge (model back, local −Z) against a wall and
    // their opening (local +Z) facing into the room. The lid swings up and back
    // over the hinge, so the chest is also pushed off that wall (below) to
    // clear it. Pick the back wall from the carved grid, preferring a Z-facing
    // wall so the long (X) side runs parallel to it.
    let chestYawDeg = 0
    let chestBackWall: 'N' | 'S' | 'W' | 'E' | null = null
    if (prop.kind === 'chest') {
      if (!carvedAt(prop.x, prop.z - 1)) chestBackWall = 'N' // wall on −Z
      else if (!carvedAt(prop.x, prop.z + 1)) chestBackWall = 'S' // wall on +Z
      else if (!carvedAt(prop.x - 1, prop.z)) chestBackWall = 'W' // wall on −X
      else if (!carvedAt(prop.x + 1, prop.z)) chestBackWall = 'E' // wall on +X
      // Map the model's back (local −Z) onto the chosen wall.
      chestYawDeg = chestBackWall ? CHEST_BACK_WALL_YAW[chestBackWall] : 0
    }

    const breakable = isBreakable(prop.kind)
    const clones: THREE.Object3D[] = []
    const count = Math.max(1, prop.stack)
    for (let i = 0; i < count; i++) {
      const clone = template.clone()

      // Yaw per kind:
      //  • chest — opening faces the room, hinge to the wall (computed above).
      //  • crate — square box: snap to 90° so it sits flush, but leave the
      //    occasional one crooked; quarter-turn each stacked tier.
      //  • barrel — cylindrical, footprint is rotation-invariant: free yaw +
      //    small per-tier twist.
      let yawDeg: number
      if (prop.kind === 'chest') {
        yawDeg = chestYawDeg
      } else if (prop.kind === 'crate') {
        const quarter = Math.round(prop.rotation / 90)
        const dev = prop.rotation - quarter * 90 // −45..45, uniform
        const tiltDeg =
          Math.abs(dev) > CRATE_ASKEW_THRESH ? dev * CRATE_ASKEW_SCALE : 0
        yawDeg = quarter * 90 + tiltDeg + i * 90
      } else {
        yawDeg = prop.rotation + i * 23
      }

      // Place within the cell. Chests get pushed off their back wall far enough
      // that the open lid clears it, with the long ends tucked off any
      // perpendicular (corner) wall. Crates push off any wall by their rotated
      // footprint overhang so they stay inside the cell; barrels are round and
      // small, so they stay centered.
      let px = prop.x + 0.5
      let pz = prop.z + 0.5
      // Barrels are round and small, so they stay centered; chests and crates
      // need their rotated footprint kept inside the cell.
      if (prop.kind !== 'barrel') {
        const aabb = rotatedRectAabb(-m.hx, m.hx, -m.hz, m.hz, (yawDeg * Math.PI) / 180)
        const insetX = Math.max(0, aabb.maxX - 0.5)
        const insetZ = Math.max(0, aabb.maxZ - 0.5)
        if (prop.kind === 'chest') {
          // Push off the back wall so the open lid clears it; tuck the long ends
          // off any perpendicular (corner) wall on the other axis.
          const backPush = Math.max(0, CHEST_LID_BACK_REACH + CHEST_LID_WALL_GAP - 0.5)
          if (chestBackWall === 'N' || chestBackWall === 'S') {
            pz += chestBackWall === 'N' ? backPush : -backPush
            if (!carvedAt(prop.x - 1, prop.z)) px += insetX
            else if (!carvedAt(prop.x + 1, prop.z)) px -= insetX
          } else if (chestBackWall === 'W' || chestBackWall === 'E') {
            px += chestBackWall === 'W' ? backPush : -backPush
            if (!carvedAt(prop.x, prop.z - 1)) pz += insetZ
            else if (!carvedAt(prop.x, prop.z + 1)) pz -= insetZ
          }
        } else {
          // Crate: push off any bordering wall by the footprint overhang.
          if (!carvedAt(prop.x - 1, prop.z)) px += insetX
          else if (!carvedAt(prop.x + 1, prop.z)) px -= insetX
          if (!carvedAt(prop.x, prop.z - 1)) pz += insetZ
          else if (!carvedAt(prop.x, prop.z + 1)) pz -= insetZ
        }
      }

      clone.position.set(px, m.seatY + i * m.height * PROP_STACK_NEST, pz)
      clone.rotation.y = (yawDeg * Math.PI) / 180
      // Interactive props (clicked → walk up → break/open). Read by
      // inputHandler's prop raycast pass. Barrels/crates break; chests open.
      const openable = prop.kind === 'chest'
      const interactive = breakable || openable
      if (interactive) {
        clone.userData.dungeonProp = true
        clone.userData.propId = index
        clone.userData.propKind = prop.kind
        clone.userData.propDepth = depth
        clone.userData.propEntranceId = dungeonManager.dungeonId
        if (breakable) clone.userData.propBreakable = true
        if (openable) clone.userData.propOpenable = true
      }
      clone.traverse((o) => {
        if (o instanceof THREE.Mesh) {
          o.castShadow = true
          o.receiveShadow = true
          if (!interactive) o.raycast = () => {} // decorative: never intercept
        }
      })
      group.add(clone)
      clones.push(clone)
    }
    entries.set(index, {
      clones,
      kind: prop.kind,
      propId: index,
      cellX: prop.x,
      cellZ: prop.z,
      rotationDeg: prop.rotation,
      broken: false,
      opened: false,
    })
  }

  /** Load + seat a kind's broken-debris variant (single, cell-centered) clone.
   *  Returns null if the variant is missing, the GLB fails to load, or the floor
   *  changed mid-load. Shared by the initial build and the live swap. */
  async function buildBrokenClone(
    kind: string,
    cellX: number,
    cellZ: number,
    rotationDeg: number,
    key: string
  ): Promise<THREE.Object3D | null> {
    const variantId = BROKEN_VARIANT[kind]
    const def = variantId ? objectManager.getCatalogEntry(variantId) : null
    if (!def) return null
    let template: THREE.Object3D
    try {
      template = (await loadGLB(`/models/objects/${def.model}`)).scene
    } catch {
      return null
    }
    if (key !== builtKey) return null
    const m = measureProp(variantId, template)
    const clone = template.clone()
    clone.position.set(cellX + 0.5, m.seatY, cellZ + 0.5)
    clone.rotation.y = (rotationDeg * Math.PI) / 180
    tagDecorative(clone)
    return clone
  }

  /** Place a prop's debris variant into `group`/`entries` (already broken at load). */
  async function addBrokenProp(
    group: THREE.Group,
    entries: Map<number, PropEntry>,
    index: number,
    prop: DungeonFloorLayout['props'][number],
    key: string
  ) {
    const clone = await buildBrokenClone(prop.kind, prop.x, prop.z, prop.rotation, key)
    if (!clone) return
    group.add(clone)
    entries.set(index, {
      clones: [clone],
      kind: prop.kind,
      propId: index,
      cellX: prop.x,
      cellZ: prop.z,
      rotationDeg: prop.rotation,
      broken: true,
    })
  }

  /**
   * Async-load and place the floor's decorative props. The dungeon-extras GLBs
   * are authored at world scale, XZ-centered with their base at Y=0, so they
   * only need seating + the cell-center offset. Props already broken (per the
   * server snapshot) render as debris directly. Committed only if the floor is
   * still current — depth/dungeon can change across the awaits; `key` is the
   * builtKey snapshot taken when the build started.
   */
  async function buildProps(
    layout: DungeonFloorLayout,
    key: string,
    depth: number
  ) {
    const specs = layout.props ?? []
    if (specs.length === 0) return
    await objectManager.fetchCatalog()
    if (key !== builtKey) return

    const group = new THREE.Group()
    group.position.set(
      dungeonManager.originX,
      dungeonManager.floorY(depth),
      dungeonManager.originZ
    )
    const entries = new Map<number, PropEntry>()

    const grid = dungeonManager.consts.grid
    const carvedAt = (x: number, z: number) =>
      x >= 0 && x < grid && z >= 0 && z < grid && layout.carved[x + z * grid]
    const broken = dungeonManager.brokenPropsForDepth(depth)

    for (let index = 0; index < specs.length; index++) {
      const prop = specs[index]
      if (broken.has(index) && isBreakable(prop.kind)) {
        await addBrokenProp(group, entries, index, prop, key)
      } else {
        await addNormalProp(group, entries, index, prop, key, depth, carvedAt)
      }
      if (key !== builtKey) return // floor changed mid-load — abandon
    }

    if (key !== builtKey) return
    clearProps()
    // Move the freshly built clones into the stable props group, matching its
    // offset to the temp group's so world positions are preserved.
    propsGroup.position.copy(group.position)
    while (group.children.length) propsGroup.add(group.children[0])
    propEntries = entries
    // Catch any breaks/opens that landed while the GLBs were loading. Chests
    // already open at build time snap to the open pose (no entrance swing).
    reconcileBrokenProps(depth, key)
    reconcileOpenedProps(depth, key, true)
  }

  /** Swap every prop the server says is broken but that's still rendered whole. */
  function reconcileBrokenProps(depth: number, key: string) {
    if (key !== builtKey) return
    const broken = dungeonManager.brokenPropsForDepth(depth)
    for (const index of broken) {
      const entry = propEntries.get(index)
      if (entry && !entry.broken) void swapPropToBroken(index, key)
    }
  }

  /** Replace one prop's whole-model clones with its broken debris variant. */
  async function swapPropToBroken(index: number, key: string) {
    const entry = propEntries.get(index)
    if (!entry || entry.broken || !isBreakable(entry.kind)) return
    entry.broken = true // claim before the await so a re-run won't double-swap
    const clone = await buildBrokenClone(
      entry.kind,
      entry.cellX,
      entry.cellZ,
      entry.rotationDeg,
      key
    )
    if (!clone) return
    for (const c of entry.clones) propsGroup.remove(c)
    propsGroup.add(clone)
    entry.clones = [clone]
  }

  /** Play the lid-open animation on every chest the server says is open but
   *  that's still rendered shut. `instant` jumps straight to the open pose (for
   *  chests already open when the floor builds); otherwise the lid animates. */
  function reconcileOpenedProps(depth: number, key: string, instant: boolean) {
    if (key !== builtKey) return
    const opened = dungeonManager.openedPropsForDepth(depth)
    for (const index of opened) {
      const entry = propEntries.get(index)
      if (entry && entry.kind === 'chest' && !entry.opened) {
        openChest(entry, instant)
      }
    }
  }

  /** FNV-1a hash of a string → 32-bit seed for `createRng` (deterministic
   *  per-prop scatter that survives reloads). */
  function hashKey(key: string): number {
    let h = 2166136261
    for (let i = 0; i < key.length; i++) {
      h ^= key.charCodeAt(i)
      h = Math.imul(h, 16777619)
    }
    return h >>> 0
  }

  async function spawnCoinSpill(entry: PropEntry, instant: boolean, key: string) {
    // openChest guards `entry.opened` before calling, so this runs once per
    // chest; the `coinSpill` check just bails on the already-spawned case.
    if (entry.coinSpill) return
    const chest = entry.clones[0]
    if (!chest) return

    let template: THREE.Object3D
    let clip: THREE.AnimationClip | null = null
    try {
      const gltf = await loadGLB(COIN_SPILL_MODEL_PATH)
      template = gltf.scene
      clip = gltf.animations[0] ?? null
    } catch {
      return
    }
    // Bail if the floor changed, the chest reverted, or a rebuild replaced this
    // entry (e.g. a debug reset landed during the load) — otherwise the clone
    // would attach to a props group it's no longer tracked in and leak.
    if (key !== builtKey || propEntries.get(entry.propId) !== entry || !entry.opened) {
      return
    }

    const clone = template.clone()
    const yaw = chest.rotation.y
    const rng = createRng(hashKey(`${key}:${entry.propId}:coin-spill`))
    const angle = ((rng() - 0.5) * COIN_SPILL_FAN_DEG * Math.PI) / 180
    const dist =
      COIN_SPILL_MIN_DIST + rng() * (COIN_SPILL_MAX_DIST - COIN_SPILL_MIN_DIST)
    const dir = yaw + angle

    // The model rests on the floor at its own origin, so seat it at the floor
    // plane (propsGroup is already offset to the floor's Y). The GLB clip bakes
    // the rise (≈+0.45m) and fall, so there's no manual height tween. The
    // spill's source is on the model's local −Z, so face −Z back toward the
    // chest (yaw = dir) to read as pouring out toward the landing spot.
    clone.position.set(
      chest.position.x + Math.sin(dir) * dist,
      0,
      chest.position.z + Math.cos(dir) * dist
    )
    clone.rotation.y = dir
    tagDecorative(clone)
    propsGroup.add(clone)
    entry.coinSpill = clone

    if (clip) {
      const mixer = new THREE.AnimationMixer(clone)
      const action = mixer.clipAction(clip)
      action.loop = THREE.LoopOnce
      action.clampWhenFinished = true
      action.play()
      if (instant) mixer.setTime(clip.duration) // already-open: jump to settled pose
      propMixers.push(mixer)
    }
  }

  /** Start (or snap to the end of) a chest's lid-open animation. Each chest gets
   *  its own mixer bound to its clone; the shared clip animates the `chest_lid`
   *  node, so cloning by name resolves correctly. */
  function openChest(entry: PropEntry, instant: boolean) {
    if (entry.opened) return
    const clone = entry.clones[0]
    if (!clone) return
    entry.opened = true
    if (chestOpenClip) {
      const mixer = new THREE.AnimationMixer(clone)
      const action = mixer.clipAction(chestOpenClip)
      action.loop = THREE.LoopOnce
      action.clampWhenFinished = true // hold the lid open after the clip ends
      action.play()
      if (instant) mixer.setTime(chestOpenClip.duration) // already-open: skip the swing
      propMixers.push(mixer)
    }
    void spawnCoinSpill(entry, instant, builtKey)
  }

  /** Cache the up-shaft sub-group's meshes + world AABB for the fade pass.
   *  `localAABB` is the group-local box from buildDungeonFloorGroup. */
  function cacheUpShaft(group: THREE.Group, localAABB: THREE.Box3) {
    const upGroup = group.getObjectByName(UP_SHAFT_GROUP_NAME)
    if (!upGroup) return
    upShaftAABB = localAABB.clone().translate(group.position)
    upGroup.traverse((obj) => {
      if (!(obj instanceof THREE.Mesh)) return
      const idx = obj.userData.textureIndex as number
      upShaftMeshes.push({
        mesh: obj,
        base: obj.material as THREE.Material,
        ghost: getGhostHousingMaterial(idx),
      })
    })
  }

  /** Cache each wall run's mesh, ghost material and world AABB for the per-run
   *  fade pass. `group.position` is set before this is called. */
  function cacheWallRuns(group: THREE.Group, runs: WallRun[]) {
    for (const r of runs) {
      const idx = r.mesh.userData.textureIndex as number
      wallRuns.push({
        mesh: r.mesh,
        base: r.mesh.material as THREE.Material,
        ghost: getGhostHousingMaterial(idx),
        aabb: r.localAABB.clone().translate(group.position),
      })
    }
  }

  function clearEntranceGroup() {
    if (entranceGroup) {
      root.remove(entranceGroup)
      disposeDungeonGroup(entranceGroup)
      entranceGroup = null
      entranceDoors = []
      doorOpenAmount = 0
      entranceVisible = false
    }
  }

  /** Write the current eased open fraction (doorOpenAmount) to both entrance
   *  door leaves' hinge rotation. Shared by the per-frame swing and the
   *  snap-to-store on (re)appear. */
  function applyDoorRotation() {
    for (const leaf of entranceDoors)
      leaf.pivot.rotation.y =
        leaf.closedAngle + (leaf.openAngle - leaf.closedAngle) * doorOpenAmount
  }

  // ── Passability debug overlay (red wireframe on blocked cell edges) ──
  // Shares the `__togglePassability` toggle with the housing overlay so one
  // command shows blocked edges everywhere. Draws the floor whose shaft the
  // player is on (the entry shaft lives in floor 1, hence Math.max(1, depth)).
  const debugPassGroup = new THREE.Group()
  debugPassGroup.name = 'dungeonPassabilityDebug'
  debugPassGroup.visible = false
  root.add(debugPassGroup)
  const debugLineMaterial = new THREE.LineBasicMaterial({ color: 0xff0000 })

  function clearDebugPass() {
    while (debugPassGroup.children.length > 0) {
      const child = debugPassGroup.children[0]
      debugPassGroup.remove(child)
      if (child instanceof THREE.LineSegments) child.geometry.dispose()
    }
  }

  function rebuildDebugPass() {
    clearDebugPass()
    if (!debugPassGroup.visible || !dungeonManager.active) return
    const floorLevel = dungeonManager.passabilityFloor(
      Math.max(1, $currentDungeonDepth)
    )
    const f = dungeonManager.floorPassabilityCells(floorLevel)
    if (!f) return

    const verts: number[] = []
    pushPassabilityEdges(
      verts,
      f.cells,
      f.width,
      f.depth,
      f.originX,
      f.originZ,
      f.yBase
    )
    if (verts.length > 0) {
      const geo = new THREE.BufferGeometry()
      geo.setAttribute('position', new THREE.Float32BufferAttribute(verts, 3))
      const lines = new THREE.LineSegments(geo, debugLineMaterial)
      lines.frustumCulled = false
      debugPassGroup.add(lines)
    }
  }

  // Toggle + re-draw on dungeon/depth change. rebuildDebugPass early-outs
  // when hidden, so this is free while the overlay is off.
  $effect(() => {
    debugPassGroup.visible = $passabilityDebugVisible
    void $currentDungeonId
    void $currentDungeonDepth
    rebuildDebugPass()
  })

  // Surface entrance structure (descending stairs + pit walls). The geometry
  // depends only on the dungeon id, so it's built once per dungeon and only
  // its visibility tracks depth: shown at depth 0, hidden underground where
  // the floor group owns the shaft (rendering both would z-fight).
  $effect(() => {
    const id = $currentDungeonId
    const depth = $currentDungeonDepth
    if ((id ?? '') !== entranceKey) {
      entranceKey = id ?? ''
      clearEntranceGroup()
      if (id) {
        const first = dungeonManager.layoutAt(1)
        if (first) {
          const c = dungeonManager.consts
          const built = buildDungeonEntranceGroup(first.upShaft, {
            grid: c.grid,
            wallHeight: c.wallHeight,
            floorHeight: c.floorHeight,
            shaftW: c.shaftW,
            shaftLen: c.shaftLen,
          })
          entranceGroup = built.group
          entranceDoors = built.doors
          doorOpenAmount = 0
          for (const leaf of built.doors)
            leaf.pivot.rotation.y = leaf.closedAngle
          entranceGroup.position.set(
            dungeonManager.originX,
            dungeonManager.entrancePos!.y,
            dungeonManager.originZ
          )
          root.add(entranceGroup)
        }
      }
    }
    if (entranceGroup) {
      const showEntrance = depth === 0
      if (showEntrance && !entranceVisible) {
        // The surface entrance just (re)appeared — returning to the surface
        // after a respawn/teleport reuses this group without a rebuild. Snap
        // the door visual to the live open/shut store so a stale open angle
        // can't linger while collision (entranceBlocksMovement) treats it as
        // shut, which would look open but block the player from descending.
        doorOpenAmount = get(dungeonDoorOpen) ? 1 : 0
        applyDoorRotation()
      }
      entranceVisible = showEntrance
      entranceGroup.visible = showEntrance
    }
  })

  $effect(() => {
    const id = $currentDungeonId
    const depth = $currentDungeonDepth
    const key = id && depth >= 1 ? `${id}:${depth}` : ''
    if (key === builtKey) return
    builtKey = key
    clearGroup()
    if (!key) return

    const layout = dungeonManager.layoutAt(depth)
    if (!layout) return
    const c = dungeonManager.consts
    const built = buildDungeonFloorGroup(layout, {
      grid: c.grid,
      wallHeight: c.wallHeight,
      floorHeight: c.floorHeight,
      shaftW: c.shaftW,
      shaftLen: c.shaftLen,
    })
    currentGroup = built.group
    currentGroup.position.set(
      dungeonManager.originX,
      dungeonManager.floorY(depth),
      dungeonManager.originZ
    )
    root.add(currentGroup)
    cacheUpShaft(currentGroup, built.upShaftAABB)
    cacheWallRuns(currentGroup, built.wallRuns)
    void buildProps(layout, key, depth)
  })

  // Reconcile prop meshes with the server's broken set whenever it changes
  // (live break broadcast, or the on-entry snapshot arriving after the build).
  $effect(() => {
    void $dungeonPropsRevision
    reconcileBrokenProps($currentDungeonDepth, builtKey)
    reconcileOpenedProps($currentDungeonDepth, builtKey, false)
  })

  // Debug reset (or any backwards authoritative snapshot): rebuild the current
  // floor's props so broken debris and open chest poses return to their normal
  // models.
  $effect(() => {
    const resetRevision = $dungeonPropsResetRevision
    if (resetRevision === lastPropsResetRevision) return
    lastPropsResetRevision = resetRevision
    if (!builtKey) return
    const depth = $currentDungeonDepth
    const layout = dungeonManager.layoutAt(depth)
    if (!layout) return
    void buildProps(layout, builtKey, depth)
  })

  onDestroy(() => {
    clearGroup()
    clearEntranceGroup()
    clearDebugPass()
    debugLineMaterial.dispose()
  })

  /** True once the player is close enough to a pending prop on the current
   *  floor to fire its break/open. */
  function pendingPropReached(
    pending: { depth: number; x: number; z: number },
    depth: number,
    playerX: number,
    playerZ: number
  ): boolean {
    if (depth !== pending.depth) return false
    const dx = playerX - pending.x
    const dz = playerZ - pending.z
    return (
      dx * dx + dz * dz <=
      PROP_INTERACT_TRIGGER_RANGE * PROP_INTERACT_TRIGGER_RANGE
    )
  }

  /** Per-frame: stair-shaft floor transitions + chest proximity. `deltaMs`
   *  advances chest lid-open and coin-spill animations. */
  export function update(
    playerX: number,
    playerY: number,
    playerZ: number,
    deltaMs = 0
  ) {
    dungeonManager.updateFromPlayerPosition(playerX, playerZ)

    // Advance one-shot GLB clips; clamped actions hold their final poses.
    if (propMixers.length > 0) {
      const dt = deltaMs / 1000
      for (const m of propMixers) m.update(dt)
    }

    // (The entrance roof has no proximity hide — it's always shown at depth 0.)

    // Fade the up-shaft stairs to a ghost when they occlude the player.
    if (upShaftAABB && upShaftMeshes.length > 0) {
      const occ = isoCameraOccludesPlayer(
        upShaftAABB,
        playerX,
        playerY,
        playerZ,
        MIN_OCCLUSION_DEPTH
      )
      if (occ !== upShaftOccluded) {
        upShaftOccluded = occ
        for (const m of upShaftMeshes) {
          m.mesh.material = occ ? m.ghost : m.base
        }
      }
    }

    // Fade each wall run that occludes the player to a ghost. The mesh's current
    // material is the single source of truth for its occluded state.
    for (const w of wallRuns) {
      const occ = isoCameraOccludesPlayer(
        w.aabb,
        playerX,
        playerY,
        playerZ,
        WALL_RUN_MIN_OCCLUSION
      )
      if (occ !== (w.mesh.material === w.ghost)) {
        w.mesh.material = occ ? w.ghost : w.base
      }
    }

    // Swing both door leaves toward their click-toggled open/shut target
    // (~0.35s either way at 60fps). Driven by dungeonDoorOpen, not proximity.
    if (entranceDoors.length > 0) {
      const target = $dungeonDoorOpen ? 1 : 0
      doorOpenAmount += (target - doorOpenAmount) * 0.12
      applyDoorRotation()
    }

    // Final-floor treasure chest: walking up to it requests an open once
    // per approach (the server validates boss state and the cooldown).
    if (!dungeonManager.active) return
    const depth = $currentDungeonDepth

    // Pending prop break: the player walked up to a barrel/crate they clicked —
    // request the break once within range. The server validates and broadcasts;
    // the visual swap happens on receipt (handles other players' breaks too).
    const pending = dungeonManager.pendingBreak
    if (pending && pendingPropReached(pending, depth, playerX, playerZ)) {
      // Reached the prop — hand off to the player swing, which breaks it at the
      // contact frame. Clear pending first so this fires once.
      const id = dungeonManager.dungeonId!
      const { depth: d, propId, x, z } = pending
      dungeonManager.clearPendingBreak()
      onPropReady?.(id, d, propId, x, z)
    }

    // Pending chest open: the player walked up to a chest they clicked — request
    // the open once within range. The server validates and broadcasts; the lid
    // animation plays on receipt (handles other players' opens too).
    const pendingOpen = dungeonManager.pendingOpen
    if (pendingOpen && pendingPropReached(pendingOpen, depth, playerX, playerZ)) {
      const id = dungeonManager.dungeonId!
      const { depth: d, propId } = pendingOpen
      dungeonManager.clearPendingOpen()
      networkManager.sendOpenDungeonProp(id, d, propId)
    }

    const layout = depth >= 1 ? dungeonManager.layoutAt(depth) : null
    const chest = layout?.chest ?? null
    if (!chest) {
      chestRequested = false
      return
    }
    const cx = dungeonManager.originX + chest[0] + 0.5
    const cz = dungeonManager.originZ + chest[1] + 0.5
    const dx = playerX - cx
    const dz = playerZ - cz
    const near = dx * dx + dz * dz < CHEST_OPEN_RANGE * CHEST_OPEN_RANGE
    if (near && !chestRequested) {
      chestRequested = true
      networkManager.sendOpenDungeonChest(dungeonManager.dungeonId!)
    } else if (!near) {
      chestRequested = false
    }
  }

  export function getGroup(): THREE.Group {
    return root
  }

  /** Raycast targets for click-to-move while underground. */
  export function getGroundMeshes(): THREE.Object3D[] {
    return currentGroup ? [currentGroup] : []
  }

  /** Raycast targets for clicking breakable props (barrels/crates). Only the
   *  breakable clones have raycast enabled, so chests/debris won't intercept. */
  export function getPropMeshes(): THREE.Object3D[] {
    return [propsGroup]
  }

  /**
   * Click raycast targets for the entrance doors. Only at depth 0 (where the
   * entrance is shown) — returns [] otherwise so closed leaves underground
   * can't intercept clicks. Reads the dungeon stores so the GameScene prop
   * re-evaluates on enter/exit and depth change.
   */
  export function getDoorMeshes(): THREE.Object3D[] {
    void $currentDungeonId
    if ($currentDungeonDepth !== 0) return []
    return entranceDoors.map((leaf) => leaf.pivot)
  }
</script>

<T is={root} />
