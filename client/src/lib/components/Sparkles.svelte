<script lang="ts">
  import * as THREE from 'three'
  import { T } from '@threlte/core'
  import { sparkTexture, type Spark } from '../utils/sparkles'

  interface Props {
    sparks: Spark[]
    /** Whether a click on a spark reaches the object it rises from. */
    clickable?: boolean
  }

  let { sparks, clickable = true }: Props = $props()

  const raycast = $derived(
    clickable ? THREE.Sprite.prototype.raycast : () => {}
  )
</script>

{#each sparks as spark, i (i)}
  <T.Sprite
    position.x={spark.x}
    position.y={spark.y}
    position.z={spark.z}
    scale={[spark.scale, spark.scale, spark.scale]}
    renderOrder={2}
    {raycast}
  >
    <T.SpriteMaterial
      map={sparkTexture()}
      color="#ffe7a8"
      transparent={true}
      opacity={spark.opacity}
      depthWrite={false}
      blending={THREE.AdditiveBlending}
    />
  </T.Sprite>
{/each}
