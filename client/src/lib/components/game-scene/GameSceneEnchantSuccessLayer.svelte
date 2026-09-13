<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
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
  import type { EnchantEffectAnchor } from '../../utils/playerEffectAnchors'
  import {
    selectEnchantLight,
    type EnchantLight,
  } from '../../utils/enchantLight'
  import {
    EnchantSuccessEffect,
    ENCHANT_SUCCESS_DURATION,
  } from '../../effects/enchant-success'

  let {
    currentPlayer,
    getAnchor,
    setPose,
  }: {
    currentPlayer: LocalPlayer | null
    getAnchor: (event: EnchantSuccess, target: EnchantEffectAnchor) => boolean
    setPose: (playerId: number, until: number, weapon: boolean) => void
  } = $props()
  const group = new THREE.Group()
  const anchor: EnchantEffectAnchor = {
    position: new THREE.Vector3(),
    weapon: null,
  }
  const frustum = new THREE.Frustum()
  const matrix = new THREE.Matrix4()
  const { camera, renderer, scene } = useThrelte()
  const slots: {
    system: EnchantSuccessEffect
    event: EnchantSuccess | null
  }[] = []
  const lights: EnchantLight[] = []
  let prepared = false

  function clear() {
    takeEnchantSuccesses()
    slots.forEach((slot) => {
      if (slot.event) setPose(slot.event.playerId, 0, slot.event.weapon)
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
    for (let i = 0; i < PREWARM_ENCHANT_EFFECTS; i++) {
      const system = new EnchantSuccessEffect()
      slots.push({ system, event: null })
      lights.push(system.light)
      group.add(system.group)
      system.group.visible = true
    }
    void (renderer as unknown as WebGPURenderer)
      .compileAsync(group, $camera, scene)
      .catch((error) =>
        console.error('Enchant success shader warmup failed', error)
      )
      .finally(() => {
        if (!cancelled) {
          clear()
          prepared = true
        }
      })
    return () => {
      cancelled = true
      clear()
      slots.forEach((slot) => slot.system.dispose())
    }
  })

  export function update() {
    if (!prepared || !$camera) return
    const player = currentPlayer
    if (!player || $teleportLoading) {
      clear()
      return
    }
    const now = Date.now()
    for (const event of takeEnchantSuccesses()) {
      if (now - event.startedAt >= ENCHANT_SUCCESS_DURATION * 1000) continue
      const local = event.playerId === player.id
      let slot = local
        ? slots[0]
        : slots.find(
            (entry, i) => i > 0 && entry.event?.playerId === event.playerId
          )
      if (!slot) {
        slot = slots.find(
          (entry, i) =>
            i > 0 &&
            (!entry.event ||
              now - entry.event.startedAt >= ENCHANT_SUCCESS_DURATION * 1000)
        )
        if (!slot) {
          const system = new EnchantSuccessEffect()
          slot = { system, event: null }
          slots.push(slot)
          lights.push(system.light)
          group.add(system.group)
        }
      }
      slot.event = event
      slot.system.light.playerId = event.playerId
      setPose(
        event.playerId,
        event.startedAt + ENCHANT_SUCCESS_DURATION * 1000,
        event.weapon
      )
    }
    matrix.multiplyMatrices(
      $camera.projectionMatrix,
      $camera.matrixWorldInverse
    )
    frustum.setFromProjectionMatrix(matrix, $camera.coordinateSystem)
    const crowded =
      slots.filter(
        (slot) =>
          slot.event &&
          now - slot.event.startedAt < ENCHANT_SUCCESS_DURATION * 1000
      ).length > 8
    for (const slot of slots) {
      const event = slot.event
      if (!event) continue
      const elapsed = (now - event.startedAt) / 1000
      const local = event.playerId === player.id
      if (
        elapsed >= ENCHANT_SUCCESS_DURATION ||
        (local && player.health <= 0)
      ) {
        setPose(event.playerId, 0, event.weapon)
        slot.event = null
        slot.system.clear()
        continue
      }
      if (
        !getAnchor(event, anchor) ||
        (!local && !frustum.containsPoint(anchor.position))
      ) {
        slot.system.clear()
        continue
      }
      slot.system.update(
        elapsed,
        anchor,
        $camera,
        crowded ||
          (!local &&
            (anchor.position.x - player.position.x) ** 2 +
              (anchor.position.z - player.position.z) ** 2 >
              225)
      )
    }
    anchor.weapon = null
  }

  export function getLight(): EnchantLight | null {
    if (!currentPlayer) return null
    return selectEnchantLight(lights, currentPlayer.id, currentPlayer.position)
  }
</script>

<T is={group} />
