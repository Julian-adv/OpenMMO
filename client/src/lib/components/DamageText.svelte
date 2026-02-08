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

  let yOffset = 2.5
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
    group.position.set(baseX, baseY + yOffset, baseZ)
    group.quaternion.copy(camera.quaternion)
  }
</script>

<T.Group bind:ref={group}>
  <Text
    {text}
    fontSize={0.3}
    {color}
    fillOpacity={opacity}
    anchorX="center"
    anchorY="middle"
  />
</T.Group>
