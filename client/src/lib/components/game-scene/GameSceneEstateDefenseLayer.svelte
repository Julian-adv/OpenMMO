<script lang="ts">
  import { T, useLoader, useThrelte } from '@threlte/core'
  import { onDestroy } from 'svelte'
  import * as THREE from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'
  import type { LocalPlayer } from '../../stores/gameStore'
  import type { Position } from '../../network/networkTypes'
  import {
    estateDefense,
    takeDefensePillars,
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

  let { currentPlayer }: { currentPlayer: LocalPlayer | null } = $props()
  const { camera, renderer, scene } = useThrelte()
  // Same loader and key as Monster.svelte, so its cache is the one warmed.
  const monsterLoader = useLoader(GLTFLoader, {
    extend: (loader) => loader.setMeshoptDecoder(MeshoptDecoder),
  })
  /** The most pillars up at once: the last wave and its leader. */
  const PILLAR_SLOTS = 13
  const CHEST_LINGER_MS = 60_000
  const group = new THREE.Group()
  const banner = new THREE.Group()
  const slots: { system: TeleportEffectSystem; event: DefensePillar | null }[] =
    []
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
    void (renderer as unknown as WebGPURenderer)
      .compileAsync(group, $camera, scene)
      .catch((error) => console.error('Summon shader warmup failed', error))
      .finally(() => slots.forEach(({ system }) => system.clear()))
    loadGLB('/models/objects/family_banner.glb')
      .then((gltf) => banner.add(gltf.scene.clone(true)))
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
  })

  function place(object: THREE.Object3D, at: Position) {
    if (currentPlayer)
      object.position.set(
        unwrapWorldXNear(currentPlayer.position.x, at.x),
        at.y,
        at.z
      )
  }

  let lastUpdate = Date.now()

  export function update() {
    const now = Date.now()
    const delta = (now - lastUpdate) / 1000
    lastUpdate = now
    if (!ready || !$camera || !currentPlayer) return
    group.visible = $currentDungeonDepth < 1 && $playerVisualFloorLevel <= 0
    const state = $estateDefense?.state
    banner.visible = !!state
    if (state) place(banner, state.banner)

    for (const pillar of takeDefensePillars()) {
      freeOrOldest(slots).event = pillar
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
