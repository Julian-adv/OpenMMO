<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import * as THREE from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import type { LocalPlayer } from '../../stores/gameStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { playerVisualFloorLevel } from '../../stores/housingStore'
  import {
    takeTeleportEffects,
    type TeleportEffect,
  } from '../../stores/teleportEffectStore'
  import { TeleportEffectSystem } from '../../effects/teleport'
  import { unwrapWorldXNear } from '../../terrain/world-wrap'

  let { currentPlayer }: { currentPlayer: LocalPlayer | null } = $props()
  const { camera, renderer, scene } = useThrelte()
  const group = new THREE.Group()
  const slots: {
    system: TeleportEffectSystem
    event: TeleportEffect | null
  }[] = []
  let prepared = false

  onMount(() => {
    let disposed = false
    for (let i = 0; i < 8; i++) {
      const system = new TeleportEffectSystem()
      slots.push({ system, event: null })
      group.add(system.group)
      system.group.visible = true
    }
    void (renderer as unknown as WebGPURenderer)
      .compileAsync(group, $camera, scene)
      .catch((error) => console.error('Teleport shader warmup failed', error))
      .finally(() => {
        if (disposed) return
        slots.forEach(({ system }) => system.clear())
        prepared = true
      })
    return () => {
      disposed = true
      slots.forEach(({ system }) => system.dispose())
    }
  })

  export function update() {
    if (!prepared || !$camera) return
    const now = Date.now()
    for (const event of takeTeleportEffects()) {
      if (event.phase === 'Cancelled') {
        for (const slot of slots) {
          if (slot.event?.playerId === event.playerId) {
            slot.event = null
            slot.system.clear()
          }
        }
        continue
      }
      const slot =
        slots.find((entry) => !entry.event) ??
        slots.reduce((oldest, entry) =>
          entry.event!.startedAt < oldest.event!.startedAt ? entry : oldest
        )
      slot.event = event
    }
    const floor =
      $currentDungeonDepth > 0
        ? -$currentDungeonDepth
        : Math.max(0, $playerVisualFloorLevel)
    for (const slot of slots) {
      const event = slot.event
      if (!event) continue
      if (
        !currentPlayer ||
        !slot.system.update(now - event.startedAt, event.phase, $camera)
      ) {
        slot.event = null
        slot.system.clear()
        continue
      }
      slot.system.group.visible = event.floorLevel === floor
      slot.system.group.position.set(
        unwrapWorldXNear(currentPlayer.position.x, event.position.x),
        event.position.y,
        event.position.z
      )
    }
  }
</script>

<T is={group} />
