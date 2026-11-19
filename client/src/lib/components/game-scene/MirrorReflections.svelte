<script lang="ts">
  import { useTask, useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import { objectDefsById } from '../../data/objectCatalog'
  import {
    mirrorPlateMaterial,
    mirrorsToReflect,
    renderMirrorReflection,
    stopMirrorReflection,
  } from '../../utils/planarMirror'

  interface Props {
    /** The viewer's character; mirrors far from it stay plain plates. */
    viewer: THREE.Vector3 | null
  }

  let { viewer }: Props = $props()

  const {
    renderer: _renderer,
    scene,
    renderStage,
    autoRenderTask,
  } = useThrelte()
  const renderer = _renderer as unknown as WebGPURenderer

  let overlay: THREE.Object3D | undefined
  let plates: THREE.Mesh[] = []
  let sinceScan = Infinity
  let skip = false

  function scan() {
    overlay ??= scene.getObjectByName('object-overlay')
    plates = []
    for (const placed of overlay?.children ?? []) {
      if (!objectDefsById.get(placed.userData.objectType)?.mirrorMaterial)
        continue
      placed.traverse((object) => {
        if (
          object instanceof THREE.Mesh &&
          object.material === mirrorPlateMaterial
        )
          plates.push(object)
      })
    }
  }

  useTask(
    (delta) => {
      sinceScan += delta
      if (sinceScan > 1) {
        sinceScan = 0
        scan()
      }
      const shown =
        viewer && plates.length > 0 ? mirrorsToReflect(plates, viewer) : []
      if (shown.length === 0) return stopMirrorReflection()
      // The view does not follow the camera, so every other frame is enough.
      skip = !skip
      if (!skip) renderMirrorReflection(renderer, scene, shown)
    },
    { stage: renderStage, before: autoRenderTask, autoInvalidate: false }
  )

  $effect(() => stopMirrorReflection)
</script>
