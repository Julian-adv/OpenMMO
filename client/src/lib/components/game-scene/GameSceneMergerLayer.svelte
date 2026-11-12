<script lang="ts">
  import { T } from '@threlte/core'
  import { onMount } from 'svelte'
  import * as THREE from 'three'
  import type { LocalPlayer } from '../../stores/gameStore'
  import {
    takeMergerCelebration,
    type MergerCelebration,
  } from '../../stores/mergerCelebrationStore'
  import { teleportLoading } from '../../stores/debugStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import {
    MergerPetals,
    MERGER_PETALS_DURATION,
    petalAreaOf,
  } from '../../effects/merger-petals'
  import { shelterAt } from '../../utils/shelterFootprints'
  import { playMergerBells } from '../../managers/sfxManager'
  import { shortestWrappedDeltaX } from '../../terrain/world-wrap'

  let { currentPlayer }: { currentPlayer: LocalPlayer | null } = $props()
  const group = new THREE.Group()
  const center = new THREE.Vector3()
  let petals: MergerPetals | null = null
  let event: MergerCelebration | null = null

  function clear() {
    takeMergerCelebration()
    event = null
    petals?.clear()
  }

  $effect(() => {
    void $currentDungeonDepth
    void $teleportLoading
    clear()
  })

  onMount(() => () => {
    clear()
    petals?.dispose()
  })

  export function update() {
    const player = currentPlayer
    if (!player) return
    const next = takeMergerCelebration()
    if (next) {
      event = next
      playMergerBells()
      if (!petals) {
        petals = new MergerPetals()
        group.add(petals.group)
      }
    }
    if (!event || !petals) return
    const elapsed = (Date.now() - event.startedAt) / 1000
    if (elapsed >= MERGER_PETALS_DURATION) {
      event = null
      petals.clear()
      return
    }
    const hall = shelterAt(event)
    const middle = hall?.placement ?? event
    center.set(
      player.position.x + shortestWrappedDeltaX(player.position.x, middle.x),
      event.y,
      middle.z
    )
    petals.update(
      elapsed,
      center,
      hall
        ? petalAreaOf(
            hall.perimeter.width,
            hall.perimeter.depth,
            hall.placement.rotation
          )
        : null
    )
  }
</script>

<T is={group} />
