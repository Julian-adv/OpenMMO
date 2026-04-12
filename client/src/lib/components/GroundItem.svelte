<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { getItemDef } from '../data/itemDefs'
  import { getWeaponModelPath } from '../utils/modelPaths'
  import { loadGLB } from '../utils/gltfCache'
  import type { GroundItemData } from '../managers/groundItemManager'

  interface Props {
    data: GroundItemData
    rotation?: number
  }

  let { data, rotation = 0 }: Props = $props()

  const def = $derived(getItemDef(data.itemDefId))
  const label = $derived(def?.name ?? data.itemDefId)

  let worldModelScene: THREE.Object3D | undefined = $state()

  function disposeObject3D(obj: THREE.Object3D) {
    obj.traverse((child) => {
      if (child instanceof THREE.Mesh) {
        child.geometry?.dispose()
        if (Array.isArray(child.material)) {
          child.material.forEach((m) => m.dispose())
        } else {
          child.material?.dispose()
        }
      }
    })
  }

  $effect(() => {
    const worldModel = def?.worldModel
    if (!worldModel) {
      worldModelScene = undefined
      return
    }
    let cancelled = false
    const path = getWeaponModelPath(worldModel)
    loadGLB(path).then((gltf) => {
      if (cancelled) return
      worldModelScene = gltf.scene.clone()
    })
    return () => {
      cancelled = true
      if (worldModelScene) {
        disposeObject3D(worldModelScene)
      }
    }
  })

  function makeNameTexture(text: string): THREE.CanvasTexture {
    const c = document.createElement('canvas')
    c.width = 256
    c.height = 64
    const ctx = c.getContext('2d')!
    ctx.fillStyle = 'rgba(0,0,0,0.6)'
    ctx.fillRect(0, 0, 256, 64)
    ctx.font = 'bold 28px Courier New'
    ctx.fillStyle = '#f0c040'
    ctx.textAlign = 'center'
    ctx.textBaseline = 'middle'
    ctx.fillText(text, 128, 32)
    return new THREE.CanvasTexture(c)
  }

  const nameTexture = $derived(worldModelScene ? null : makeNameTexture(label))
</script>

<T.Group
  position.x={data.position.x}
  position.y={data.position.y + 0.3}
  position.z={data.position.z}
  rotation.y={worldModelScene ? 0 : rotation}
  userData={{ groundItemId: data.instanceId }}
>
  {#if worldModelScene}
    <T is={worldModelScene} />
  {:else}
    <T.Mesh>
      <T.BoxGeometry args={[0.3, 0.3, 0.3]} />
      <T.MeshStandardMaterial color="#f0c040" emissive="#f0c040" emissiveIntensity={0.3} />
    </T.Mesh>

    {#if nameTexture}
      <T.Sprite position.y={0.5} scale={[label.length * 0.08, 0.2, 1]}>
        <T.SpriteMaterial map={nameTexture} transparent={true} />
      </T.Sprite>
    {/if}
  {/if}
</T.Group>
