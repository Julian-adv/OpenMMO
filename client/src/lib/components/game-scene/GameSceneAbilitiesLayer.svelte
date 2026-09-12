<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import * as THREE from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import {
    SwordGuardEffect,
    GUARD_EFFECT_DURATION,
  } from '../../effects/sword-guard'
  import {
    takeAbilityEffects,
    type AbilityEffectEvent,
  } from '../../stores/abilityStore'
  import { teleportLoading } from '../../stores/debugStore'
  import { unwrapWorldXNear } from '../../terrain/world-wrap'
  import type { LocalPlayer } from '../../stores/gameStore'
  import {
    playAbilitySound,
    preloadAbilitySounds,
  } from '../../managers/sfxManager'

  let {
    currentPlayer,
    floorLevel,
    getAnchor,
  }: {
    currentPlayer: LocalPlayer | null
    floorLevel: number
    getAnchor: (id: number, shield: boolean, target: THREE.Vector3) => boolean
  } = $props()

  const { camera, renderer, scene } = useThrelte()
  const group = new THREE.Group()
  const anchor = new THREE.Vector3()
  const shield = new THREE.Vector3()
  const slots: {
    effect: SwordGuardEffect
    event: AbilityEffectEvent | null
  }[] = []
  let prepared = false

  function createSlot() {
    const effect = new SwordGuardEffect(
      Array.from({ length: 5 }, () => new THREE.Vector3())
    )
    group.add(effect.group)
    const slot = { effect, event: null as AbilityEffectEvent | null }
    slots.push(slot)
    return slot
  }

  function clear() {
    takeAbilityEffects()
    for (const slot of slots) {
      slot.event = null
      slot.effect.group.visible = false
    }
  }

  $effect(() => {
    void floorLevel
    void $teleportLoading
    clear()
  })

  onMount(() => {
    let cancelled = false
    preloadAbilitySounds()
    for (let i = 0; i < 2; i++) createSlot()
    void (async () => {
      try {
        await (renderer as unknown as WebGPURenderer).compileAsync(
          group,
          $camera,
          scene
        )
      } catch (error) {
        console.error('Guardian Ward shader warmup failed', error)
      } finally {
        if (!cancelled) {
          clear()
          prepared = true
        }
      }
    })()
    return () => {
      cancelled = true
      clear()
      for (const slot of slots) slot.effect.dispose()
    }
  })

  useTask(() => {
    if (!prepared) return
    if (!currentPlayer || $teleportLoading) {
      clear()
      return
    }
    const now = Date.now()
    for (const event of takeAbilityEffects()) {
      if (event.floor_level !== floorLevel || now - event.startedAt > 1000)
        continue
      const slot =
        slots.find((slot) => !slot.event) ??
        (slots.length < 8 ? createSlot() : null)
      if (!slot) continue
      slot.event = event
      const origin = slot.effect.group.position
      origin.set(
        unwrapWorldXNear(currentPlayer.position.x, event.position.x),
        event.position.y,
        event.position.z
      )
      if (
        Math.hypot(
          origin.x - currentPlayer.position.x,
          origin.z - currentPlayer.position.z
        ) <= 20
      )
        playAbilitySound(event.ability)
      slot.effect.group.visible = true
      for (let i = 0; i < 5; i++) {
        const id = event.targets[i]
        const found = id !== undefined && getAnchor(id, false, anchor)
        slot.effect.setTarget(i, found ? anchor.sub(origin) : null, true)
      }
    }
    for (const slot of slots) {
      const event = slot.event
      if (!event) continue
      const elapsed = (now - event.startedAt) / 1000
      if (
        elapsed >= GUARD_EFFECT_DURATION ||
        event.floor_level !== floorLevel
      ) {
        slot.event = null
        slot.effect.group.visible = false
        continue
      }
      const origin = slot.effect.group.position
      for (let i = 0; i < 5; i++) {
        const id = event.targets[i]
        slot.effect.setTarget(
          i,
          id !== undefined && getAnchor(id, false, anchor)
            ? anchor.sub(origin)
            : null
        )
      }
      const hasShield = getAnchor(event.player_id, true, shield)
      slot.effect.update(
        elapsed,
        20,
        $camera,
        hasShield ? shield.sub(origin) : null
      )
    }
  })
</script>

<T is={group} />
