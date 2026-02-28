<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { onMount } from 'svelte'
  import { hoveredCell } from '../../stores/editorStore'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'
  import type { TerrainTile } from '../game-scene/terrain-utils'

  interface Props {
    camera: THREE.OrthographicCamera | undefined
    terrainMeshes: (THREE.Mesh | undefined)[]
    terrainTiles: TerrainTile[]
  }

  let { camera, terrainMeshes, terrainTiles: _terrainTiles }: Props = $props()

  let cursorPosition = $state<[number, number, number] | null>(null)

  const raycaster = new THREE.Raycaster()
  const mouseNDC = new THREE.Vector2()

  function handleMouseMove(event: MouseEvent) {
    if (!camera) return

    const meshes = terrainMeshes.filter((m): m is THREE.Mesh => m !== undefined)
    if (meshes.length === 0) return

    const rect = (event.target as HTMLElement).getBoundingClientRect()
    mouseNDC.set(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )

    raycaster.setFromCamera(mouseNDC, camera)
    const intersects = raycaster.intersectObjects(meshes, false)

    if (intersects.length === 0) {
      hoveredCell.set(null)
      cursorPosition = null
      return
    }

    const hit = intersects[0]
    const mesh = hit.object as THREE.Mesh

    // Local offset within tile mesh (-32 ~ +32)
    const localX = hit.point.x - mesh.position.x
    const localZ = hit.point.z - mesh.position.z

    // Cell coordinates (0 ~ 63)
    const cellX = Math.max(0, Math.min(63, Math.floor(localX + TERRAIN_TILE_SIZE / 2)))
    const cellZ = Math.max(0, Math.min(63, Math.floor(localZ + TERRAIN_TILE_SIZE / 2)))

    // Tile coordinates
    const tileX = Math.round(mesh.position.x / TERRAIN_TILE_SIZE)
    const tileZ = Math.round(mesh.position.z / TERRAIN_TILE_SIZE)

    // Cell center in world space
    const worldX = mesh.position.x - TERRAIN_TILE_SIZE / 2 + cellX + 0.5
    const worldZ = mesh.position.z - TERRAIN_TILE_SIZE / 2 + cellZ + 0.5

    hoveredCell.set({ tileX, tileZ, cellX, cellZ, worldX, worldZ })
    cursorPosition = [worldX, 0.05, worldZ]
  }

  function handleMouseOut() {
    hoveredCell.set(null)
    cursorPosition = null
  }

  onMount(() => {
    const canvas = document.querySelector('canvas')
    if (!canvas) return

    canvas.addEventListener('mousemove', handleMouseMove)
    canvas.addEventListener('mouseleave', handleMouseOut)

    return () => {
      canvas.removeEventListener('mousemove', handleMouseMove)
      canvas.removeEventListener('mouseleave', handleMouseOut)
      hoveredCell.set(null)
    }
  })
</script>

{#if cursorPosition}
  <T.Mesh position={cursorPosition} rotation.x={-Math.PI / 2}>
    <T.PlaneGeometry args={[1, 1]} />
    <T.MeshBasicMaterial
      color="#ffffff"
      transparent
      opacity={0.35}
      side={THREE.DoubleSide}
      depthWrite={false}
    />
  </T.Mesh>
{/if}
