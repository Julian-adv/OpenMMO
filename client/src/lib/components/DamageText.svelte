<script lang="ts">
  import { T } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import * as THREE from 'three'

  interface Props {
    text: string
    color: string
  }

  let { text, color }: Props = $props()
  let group = $state<THREE.Group | undefined>(undefined)

  let yOffset = $state(1.8)
  let life = 1.0
  let opacity = $state(1)

  let _alive = true

  export function isAlive() {
    return _alive
  }

  export function update(
    deltaTime: number,
    baseX: number,
    baseY: number,
    baseZ: number,
    camera: THREE.Camera
  ) {
    life -= deltaTime
    yOffset += deltaTime * 1.5
    opacity = Math.max(0, Math.min(1, life * 2))
    _alive = life > 0

    if (!group) return
    // Position at monster base, face camera
    group.position.set(baseX, baseY, baseZ)
    group.quaternion.copy(camera.quaternion)
    // yOffset is applied in local space via the Text's position.y prop,
    // so it moves in screen-up direction (billboard local Y)
  }
</script>

<!-- Outer group: billboard at monster position -->
<T.Group bind:ref={group}>
  <!-- Inner offset: local Y = screen up -->
  <Text
    {text}
    fontSize={0.25}
    {color}
    fillOpacity={opacity}
    position.y={yOffset}
    anchorX="center"
    anchorY="middle"
  />
</T.Group>
