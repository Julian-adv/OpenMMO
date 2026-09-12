<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import * as THREE from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import type { LocalPlayer } from '../../stores/gameStore'
  import {
    PREWARM_ENCHANT_EFFECTS,
    takeEnchantSuccesses,
    type EnchantSuccess,
  } from '../../stores/enchantSuccessStore'
  import { teleportLoading } from '../../stores/debugStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { EnchantSuccessEffect } from '../../effects/enchant-success'

  let {
    currentPlayer,
    getAnchor,
  }: {
    currentPlayer: LocalPlayer | null
    getAnchor: (event: EnchantSuccess, target: THREE.Vector3) => boolean
  } = $props()
  const group = new THREE.Group()
  const anchor = new THREE.Vector3()
  const frustum = new THREE.Frustum()
  const matrix = new THREE.Matrix4()
  const { camera, renderer, scene } = useThrelte()
  const slots: {
    system: EnchantSuccessEffect
    event: EnchantSuccess | null
  }[] = []
  let prepared = false
  let textureMap: THREE.Texture | null = null

  function clear() {
    takeEnchantSuccesses()
    slots.forEach((slot) => {
      slot.event = null
      slot.system.clear()
    })
  }

  $effect(() => {
    void $currentDungeonDepth
    void $teleportLoading
    clear()
  })

  onMount(() => {
    let cancelled = false
    const map = new THREE.TextureLoader().load(
      '/textures/vfx/enchant-filament.png',
      async () => {
        if (cancelled) return
        map.colorSpace = THREE.SRGBColorSpace
        textureMap = map
        for (let i = 0; i < PREWARM_ENCHANT_EFFECTS; i++) {
          const system = new EnchantSuccessEffect(map)
          slots.push({ system, event: null })
          group.add(system.group)
          if (i === 0) group.add(system.light)
          system.group.visible = true
        }
        try {
          await (renderer as unknown as WebGPURenderer).compileAsync(
            group,
            $camera,
            scene
          )
        } catch (error) {
          console.error('Enchant success shader warmup failed', error)
        } finally {
          if (!cancelled) {
            clear()
            prepared = true
          }
        }
      },
      undefined,
      (error) => console.error('Enchant success texture failed to load', error)
    )
    return () => {
      cancelled = true
      clear()
      slots.forEach((slot) => slot.system.dispose())
      map.dispose()
    }
  })

  useTask(() => {
    if (!prepared || !$camera) return
    const player = currentPlayer
    if (!player || $teleportLoading) {
      clear()
      return
    }
    const now = Date.now()
    for (const event of takeEnchantSuccesses()) {
      if (now - event.startedAt >= 600) continue
      const local = event.playerId === player.id
      let slot = local
        ? slots[0]
        : slots.find(
            (entry, i) => i > 0 && entry.event?.playerId === event.playerId
          )
      if (!slot) {
        slot = slots.find(
          (entry, i) =>
            i > 0 && (!entry.event || now - entry.event.startedAt >= 600)
        )
        if (!slot && textureMap) {
          const system = new EnchantSuccessEffect(textureMap)
          slot = { system, event: null }
          slots.push(slot)
          group.add(system.group)
        }
      }
      if (slot) slot.event = event
    }
    matrix.multiplyMatrices(
      $camera.projectionMatrix,
      $camera.matrixWorldInverse
    )
    frustum.setFromProjectionMatrix(matrix, $camera.coordinateSystem)
    const crowded =
      slots.filter((slot) => slot.event && now - slot.event.startedAt < 600)
        .length > 8
    for (const slot of slots) {
      const event = slot.event
      if (!event) continue
      const elapsed = (now - event.startedAt) / 1000
      const local = event.playerId === player.id
      if (elapsed >= 0.6 || (local && player.health <= 0)) {
        slot.event = null
        slot.system.clear()
        continue
      }
      if (
        !getAnchor(event, anchor) ||
        (!local && !frustum.containsPoint(anchor))
      ) {
        slot.system.clear()
        continue
      }
      slot.system.update(
        Math.min(1, elapsed / 0.3),
        elapsed < 0.3 ? null : (elapsed - 0.3) * 2,
        anchor,
        $camera,
        crowded ||
          (!local &&
            (anchor.x - player.position.x) ** 2 +
              (anchor.z - player.position.z) ** 2 >
              225)
      )
    }
  })
</script>

<T is={group} />
