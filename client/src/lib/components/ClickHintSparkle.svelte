<script lang="ts">
  import { T, useTask } from '@threlte/core'
  import { createRng } from '../utils/simplex-noise'
  import { sparkSeed, sparksAt } from '../utils/sparkles'
  import Sparkles from './Sparkles.svelte'

  /** Rising sparkles over a map object the player is meant to click. */
  interface Props {
    x: number
    y: number
    z: number
    /** Yaw in degrees. */
    rotation: number
    /** Local box the sparks rise from: half width, height, half depth. */
    size?: [number, number, number]
  }

  let { x, y, z, rotation, size = [0.7, 1.3, 0.35] }: Props = $props()

  const COUNT = 44
  const LOOK = { lifetime: 1.6, rise: 0.6, size: 0.16 }

  const seeds = $derived.by(() => {
    const rand = createRng(Math.round(x * 31 + z * 17) >>> 0)
    const yaw = (rotation * Math.PI) / 180
    return Array.from({ length: COUNT }, () => {
      const lx = size[0] * (rand() * 2 - 1)
      const lz = size[2] * (rand() * 2 - 1)
      return sparkSeed(rand, [lx, size[1] * (0.1 + 0.9 * rand()), lz], yaw)
    })
  })

  let seconds = $state(0)
  useTask((delta) => {
    seconds += delta
  })
</script>

<T.Group position.x={x} position.y={y} position.z={z}>
  <Sparkles sparks={sparksAt(seeds, seconds, LOOK)} clickable={false} />
</T.Group>
