<script lang="ts">
  import { T, useLoader, useThrelte } from '@threlte/core'
  import { onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import * as THREE from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'
  import type { LocalPlayer } from '../../stores/gameStore'
  import type { Position } from '../../network/networkTypes'
  import {
    bannerFall,
    bannerStruckAt,
    estateDefense,
    incomingAngles,
    takeDefensePillars,
    type EstateDefenseState,
    type DefensePillar,
  } from '../../stores/estateDefenseStore'
  import { playerVisualFloorLevel } from '../../stores/housingStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import {
    freeOrOldest,
    SUMMON_PALETTE,
    TeleportEffectSystem,
  } from '../../effects/teleport'
  import { loadGLB } from '../../utils/gltfCache'
  import { bindClipOnce } from '../../utils/clipOnce'
  import { preloadMonsterTypes } from '../../utils/monsterPreload'
  import { playPropSound } from '../../managers/sfxManager'
  import { unwrapWorldXNear } from '../../terrain/world-wrap'
  import { EarthRoarEffect } from '../../effects/earth-roar'
  import { emblemClothMaterial, type Emblem } from '../../utils/familyEmblem'
  import { EARTH_ROAR_STRIKE_MS } from '../../data/monsterSkills'

  let { currentPlayer }: { currentPlayer: LocalPlayer | null } = $props()
  const { camera, renderer, scene, size } = useThrelte()
  // Same loader and key as Monster.svelte, so its cache is the one warmed.
  const monsterLoader = useLoader(GLTFLoader, {
    extend: (loader) => loader.setMeshoptDecoder(MeshoptDecoder),
  })
  /** The most pillars up at once: the last wave and its leader. */
  const PILLAR_SLOTS = 13
  const CHEST_LINGER_MS = 60_000
  /** A leader's rising shakes the ground this far around it. */
  const LEADER_ROAR_REACH = 5
  const LEADER_FOOTPRINT = { halfWidth: 0.8, front: 0.8, back: 0.8 }
  const BANNER_SHAKE_MS = 500
  /** A lost banner tips over, then sinks out of sight. */
  const BANNER_TIP_MS = 600
  const BANNER_SINK_MS = 900
  const BANNER_SINK_M = 1.5
  const group = new THREE.Group()
  const banner = new THREE.Group()
  /** Scratch for placing a server position near the player. */
  const dummy = new THREE.Object3D()
  const slots: { system: TeleportEffectSystem; event: DefensePillar | null }[] =
    []
  let leaderRoar: EarthRoarEffect | null = null
  let bannerCloth: THREE.Mesh | null = null
  let bannerMaterial: THREE.Material | null = null
  let bannerKey: string | null = null
  let chestModel: THREE.Object3D | null = null
  let chestOpenClip: THREE.AnimationClip | null = null
  const chests: {
    pillar: DefensePillar
    model: THREE.Object3D
    mixer: THREE.AnimationMixer | null
  }[] = []
  let ready = false
  let preloadedFor: number | null = null

  /** Built on the first defense seen, ahead of its first wave. */
  function prepare() {
    ready = true
    for (let i = 0; i < PILLAR_SLOTS; i++) {
      const system = new TeleportEffectSystem(SUMMON_PALETTE)
      slots.push({ system, event: null })
      group.add(system.group)
    }
    group.add(banner)
    leaderRoar = new EarthRoarEffect(LEADER_ROAR_REACH, LEADER_FOOTPRINT)
    group.add(leaderRoar.group)
    // Hidden objects are skipped by the warmup compile.
    leaderRoar.group.visible = true
    void (renderer as unknown as WebGPURenderer)
      .compileAsync(group, $camera, scene)
      .catch((error) => console.error('Summon shader warmup failed', error))
      .finally(() => {
        slots.forEach(({ system }) => system.clear())
        if (leaderRoar) leaderRoar.group.visible = false
      })
    loadGLB('/models/objects/family_banner.glb')
      .then((gltf) => {
        const model = gltf.scene.clone(true)
        bannerCloth = model.getObjectByName('emblem_cloth') as THREE.Mesh | null
        dressBanner(get(estateDefense)?.state.emblem)
        banner.add(model)
      })
      .catch((error) => console.error('Banner model failed to load', error))
    loadGLB('/models/objects/chest_animated.glb')
      .then((gltf) => {
        chestModel = gltf.scene
        chestOpenClip =
          gltf.animations.find((clip) => clip.name === 'ChestOpen') ?? null
      })
      .catch((error) => console.error('Chest model failed to load', error))
  }

  const unsubscribe = estateDefense.subscribe((current) => {
    if (current) dressBanner(current.state.emblem)
    if (!current || preloadedFor === current.state.estate_id) return
    preloadedFor = current.state.estate_id
    if (!ready) prepare()
    preloadMonsterTypes(current.state.monsters, (url) =>
      monsterLoader.load(url)
    )
  })
  onDestroy(() => {
    unsubscribe()
    slots.forEach(({ system }) => system.dispose())
    leaderRoar?.dispose()
    bannerMaterial?.dispose()
  })

  /** The estate's family emblem on the banner, plain without a family. */
  function dressBanner(emblem: Emblem | null | undefined) {
    const key = emblem ? `${emblem.texture}/${emblem.color}` : ''
    if (!bannerCloth || key === bannerKey) return
    bannerMaterial?.dispose()
    bannerCloth.material = bannerMaterial = emblemClothMaterial(emblem)
    bannerKey = key
  }

  function place(object: THREE.Object3D, at: Position) {
    if (currentPlayer)
      object.position.set(
        unwrapWorldXNear(currentPlayer.position.x, at.x),
        at.y,
        at.z
      )
  }

  let lastUpdate = Date.now()
  const projectedFrom = new THREE.Vector3()
  const projectedBanner = new THREE.Vector3()
  /** The last wave comes from every side. */
  const EVERY_SIDE = [Math.PI / 2, 0, -Math.PI / 2, Math.PI]
  let shown: number | 'every side' | null = null

  /** The next wave's direction on screen, which follows the camera. */
  function updateIncomingAngles(state: EstateDefenseState | undefined) {
    if (!state || state.next_wave_in_ms == null) {
      if (shown !== null) incomingAngles.set([])
      shown = null
      return
    }
    if (!state.next_from) {
      if (shown !== 'every side') incomingAngles.set(EVERY_SIDE)
      shown = 'every side'
      return
    }
    projectedBanner.copy(banner.position).project($camera)
    place(dummy, state.next_from)
    projectedFrom.copy(dummy.position).project($camera)
    const angle = Math.atan2(
      (projectedFrom.y - projectedBanner.y) * size.current.height,
      (projectedFrom.x - projectedBanner.x) * size.current.width
    )
    if (typeof shown !== 'number' || Math.abs(angle - shown) > 0.02) {
      shown = angle
      incomingAngles.set([angle])
    }
  }

  export function update() {
    const now = Date.now()
    const delta = (now - lastUpdate) / 1000
    lastUpdate = now
    if (!ready || !$camera || !currentPlayer) return
    group.visible = $currentDungeonDepth < 1 && $playerVisualFloorLevel <= 0
    const state = $estateDefense?.state
    const sinceFall = bannerFall ? now - bannerFall.at : Infinity
    banner.visible = !!state || sinceFall < BANNER_TIP_MS + BANNER_SINK_MS
    if (state) {
      place(banner, state.banner)
      const sinceBlow = now - bannerStruckAt
      banner.rotation.z =
        sinceBlow >= 0 && sinceBlow < BANNER_SHAKE_MS
          ? Math.sin(sinceBlow / 25) * 0.1 * (1 - sinceBlow / BANNER_SHAKE_MS)
          : 0
    } else if (bannerFall && banner.visible) {
      place(banner, bannerFall.position)
      const tip = Math.min(1, sinceFall / BANNER_TIP_MS)
      banner.rotation.z = (Math.PI / 2) * 0.9 * tip * tip
      banner.position.y -=
        BANNER_SINK_M *
        Math.max(0, (sinceFall - BANNER_TIP_MS) / BANNER_SINK_MS)
    }
    updateIncomingAngles(state)

    for (const pillar of takeDefensePillars()) {
      freeOrOldest(slots).event = pillar
      if (pillar.leader && leaderRoar) {
        // Joined late in its gathering, so the ground breaks as it appears.
        place(dummy, pillar.position)
        leaderRoar.start(
          dummy.position,
          Math.max(
            0,
            EARTH_ROAR_STRIKE_MS - pillar.riseMs + now - pillar.startedAt
          )
        )
      }
      if (pillar.chest && chestModel) {
        const model = chestModel.clone(true)
        place(model, pillar.position)
        model.visible = false
        group.add(model)
        chests.push({ pillar, model, mixer: null })
      }
    }
    for (let i = chests.length - 1; i >= 0; i--) {
      const chest = chests[i]
      const age = now - chest.pillar.startedAt
      if (age >= CHEST_LINGER_MS) {
        group.remove(chest.model)
        chests.splice(i, 1)
        continue
      }
      chest.model.visible = age >= chest.pillar.riseMs / 2
      if (!chest.mixer && age >= chest.pillar.riseMs && chestOpenClip) {
        chest.mixer = bindClipOnce(chest.model, chestOpenClip, false)
        playPropSound('chestOpen')
      }
      chest.mixer?.update(delta)
    }
    leaderRoar?.update(delta, $camera)
    for (const slot of slots) {
      const pillar = slot.event
      if (!pillar) continue
      if (!slot.system.summon(now - pillar.startedAt, pillar.riseMs, $camera)) {
        slot.event = null
        continue
      }
      place(slot.system.group, pillar.position)
    }
  }
</script>

<T is={group} />
