<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import * as THREE from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import type { LocalPlayer } from '../../stores/gameStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { playerVisualFloorLevel } from '../../stores/housingStore'
  import {
    takeGroundEruptions,
    type GroundEruption,
  } from '../../stores/groundEruptionStore'
  import {
    GroundEruptionAssets,
    GroundEruptionEffect,
  } from '../../effects/ground-eruption'
  import { unwrapWorldXNear } from '../../terrain/world-wrap'
  import { playMonsterSound } from '../../managers/sfxManager'

  let { currentPlayer }: { currentPlayer: LocalPlayer | null } = $props()
  const { camera, renderer, scene } = useThrelte()
  const group = new THREE.Group()
  const slots: {
    effect: GroundEruptionEffect
    event: GroundEruption | null
    erupted: boolean
  }[] = []
  let prepared = false
  let seed = 1

  onMount(() => {
    let disposed = false
    const assets = new GroundEruptionAssets()
    // An earthquake's marks, a smash's pools and a few ground strikes at once.
    for (let i = 0; i < 16; i++) {
      const effect = new GroundEruptionEffect(assets)
      slots.push({ effect, event: null, erupted: false })
      group.add(effect.group)
      // Odd slots warm the crystal shards' shader too.
      effect.start(1, 1.5, i + 1, 0, i % 2 === 1)
    }
    void (renderer as unknown as WebGPURenderer)
      .compileAsync(group, $camera, scene)
      .catch((error) =>
        console.error('Ground eruption shader warmup failed', error)
      )
      .finally(() => {
        if (disposed) return
        slots.forEach(({ effect }) => effect.clear())
        prepared = true
      })
    return () => {
      disposed = true
      slots.forEach(({ effect }) => effect.dispose())
      assets.dispose()
    }
  })

  export function update() {
    if (!prepared || !$camera) return
    const now = Date.now()
    for (const event of takeGroundEruptions()) {
      const slot =
        slots.find((entry) => !entry.event) ??
        slots.reduce((oldest, entry) =>
          entry.event!.startedAt < oldest.event!.startedAt ? entry : oldest
        )
      slot.event = event
      slot.erupted = false
      slot.effect.start(
        event.impactMs,
        event.radius,
        seed++,
        event.lingerMs,
        event.crystal
      )
    }
    const floor =
      $currentDungeonDepth > 0
        ? -$currentDungeonDepth
        : Math.max(0, $playerVisualFloorLevel)
    // An earthquake's marks split open together; they erupt as one sound.
    const sounds: string[] = []
    for (const slot of slots) {
      const event = slot.event
      if (!event) continue
      const elapsed = now - event.startedAt
      if (!currentPlayer || !slot.effect.update(elapsed, $camera)) {
        slot.event = null
        slot.effect.clear()
        continue
      }
      if (!slot.erupted && elapsed >= event.impactMs) {
        slot.erupted = true
        const { sound } = event
        if (sound && event.floorLevel === floor && !sounds.includes(sound)) {
          sounds.push(sound)
        }
      }
      slot.effect.group.visible = event.floorLevel === floor
      slot.effect.group.position.set(
        unwrapWorldXNear(currentPlayer.position.x, event.position.x),
        event.position.y,
        event.position.z
      )
    }
    for (const sound of sounds) playMonsterSound(sound)
  }
</script>

<T is={group} />
