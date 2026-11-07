<script lang="ts" module>
  import * as THREE from 'three'

  export const RESOURCE_BAR_WIDTH = 1.0
  export const RESOURCE_BAR_HEIGHT = 0.08
  const TEMP_HEALTH_COLOR = '#faf089'

  const backGeometry = new THREE.PlaneGeometry(
    RESOURCE_BAR_WIDTH,
    RESOURCE_BAR_HEIGHT
  )
  // Left-anchored so a fill only needs scale.x to grow or shrink.
  const fillGeometry = new THREE.PlaneGeometry(
    RESOURCE_BAR_WIDTH,
    RESOURCE_BAR_HEIGHT
  )
  fillGeometry.translate(RESOURCE_BAR_WIDTH / 2, 0, 0)
</script>

<script lang="ts">
  import { T } from '@threlte/core'

  interface Props {
    y: number
    ratio: number
    color: string
    shieldRatio?: number
    /** Drawn over the scene, for the local player's own bars. */
    onTop?: boolean
    renderOrder?: number
  }

  let {
    y,
    ratio,
    color,
    shieldRatio = 0,
    onTop = false,
    renderOrder = 0,
  }: Props = $props()
</script>

<T.Group position.y={y} {renderOrder}>
  <T.Mesh>
    <T is={backGeometry} />
    <T.MeshBasicMaterial
      color="#000000"
      transparent
      opacity={0.5}
      depthTest={!onTop}
      depthWrite={false}
    />
  </T.Mesh>
  {#if ratio > 0}
    <T.Mesh
      position.x={-RESOURCE_BAR_WIDTH / 2}
      position.z={0.001}
      scale.x={ratio}
      renderOrder={1}
    >
      <T is={fillGeometry} />
      <T.MeshBasicMaterial
        {color}
        transparent
        depthTest={!onTop}
        depthWrite={false}
      />
    </T.Mesh>
  {/if}
  {#if shieldRatio > 0}
    <T.Mesh
      position.x={RESOURCE_BAR_WIDTH * (ratio - 0.5)}
      position.z={0.002}
      scale.x={shieldRatio}
      renderOrder={2}
    >
      <T is={fillGeometry} />
      <T.MeshBasicMaterial
        color={TEMP_HEALTH_COLOR}
        transparent
        depthTest={false}
        depthWrite={false}
      />
    </T.Mesh>
  {/if}
</T.Group>
