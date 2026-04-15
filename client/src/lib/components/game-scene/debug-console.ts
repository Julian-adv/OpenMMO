import type { WebGPURenderer } from 'three/webgpu'
import type * as THREE from 'three'
import type { LoopProfiler } from './loop-profiler'
import type { Writable } from 'svelte/store'
import { passabilityDebugVisible } from '../../stores/debugStore'
import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
import { TERRAIN_TILE_SIZE, worldToTileCell } from './terrain-utils'
import { readCell } from '../../terrain/splat-encoding'

export interface DebugConsoleDeps {
  loopProfiler: LoopProfiler
  getLoopProfileEnabled: () => boolean
  setLoopProfileEnabled: (enabled: boolean) => void
  renderer: WebGPURenderer
  scene: THREE.Scene
  getGrassGroup: () => THREE.Group | undefined
  getHousingGroup: () => THREE.Group | undefined
  getTerrainGroup: () => THREE.Group | undefined
  refractionEnabled: Writable<boolean>
  reflectionEnabled: Writable<boolean>
  splatManager: TerrainSplatManager
  getPlayerPos: () => { x: number; z: number } | null
}

const DEBUG_KEYS = [
  '__profile',
  '__ri',
  '__toggleGrass',
  '__toggleHousing',
  '__toggleRefraction',
  '__toggleReflection',
  '__toggleTerrain',
  '__togglePassability',
  '__countMeshes',
  '__inspectSplat',
] as const

export function registerDebugConsole(
  getDeps: () => DebugConsoleDeps
): () => void {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const w = window as any

  w.__profile = () => {
    const deps = getDeps()
    const next = !deps.getLoopProfileEnabled()
    deps.setLoopProfileEnabled(next)
    if (next) {
      deps.loopProfiler.resetWindow(performance.now())
      console.log('[LoopProfile] STARTED — stats will print every 1s')
    } else {
      console.log('[LoopProfile] STOPPED')
    }
  }

  w.__ri = () => {
    const deps = getDeps()
    const r = deps.renderer.info.render
    console.log(
      `[RendererInfo] calls=${r.calls} tris=${r.triangles} points=${r.points} lines=${r.lines}`
    )
    console.log('[RendererInfo] raw:', deps.renderer.info)
  }

  w.__toggleGrass = () => {
    const g = getDeps().getGrassGroup()
    if (g) {
      g.visible = !g.visible
      console.log(`[Toggle] grass visible=${g.visible}`)
    }
  }

  w.__toggleHousing = () => {
    const g = getDeps().getHousingGroup()
    if (g) {
      g.visible = !g.visible
      console.log(`[Toggle] housing visible=${g.visible}`)
    }
  }

  w.__toggleRefraction = () => {
    getDeps().refractionEnabled.update((v: boolean) => !v)
    console.log(`[Toggle] refraction`)
  }

  w.__toggleReflection = () => {
    getDeps().reflectionEnabled.update((v: boolean) => !v)
    console.log(`[Toggle] reflection`)
  }

  w.__togglePassability = () => {
    passabilityDebugVisible.update((v: boolean) => !v)
    console.log(`[Toggle] passability debug`)
  }

  w.__toggleTerrain = () => {
    const g = getDeps().getTerrainGroup()
    if (g) {
      g.visible = !g.visible
      console.log(`[Toggle] terrain visible=${g.visible}`)
    }
  }

  w.__countMeshes = () => {
    const deps = getDeps()
    let meshCount = 0
    let instancedCount = 0
    let totalTris = 0
    let totalInstances = 0
    deps.scene.traverse((obj: THREE.Object3D) => {
      if (!obj.visible) return
      if ((obj as THREE.InstancedMesh).isInstancedMesh) {
        const im = obj as THREE.InstancedMesh
        instancedCount++
        totalInstances += im.count
        const geo = im.geometry
        const idxCount = geo.index
          ? geo.index.count
          : geo.attributes.position.count
        totalTris += (idxCount / 3) * im.count
      } else if ((obj as THREE.Mesh).isMesh) {
        meshCount++
        const geo = (obj as THREE.Mesh).geometry
        const idxCount = geo.index
          ? geo.index.count
          : geo.attributes.position.count
        totalTris += idxCount / 3
      }
    })
    console.log(
      `[SceneCount] meshes=${meshCount} instanced=${instancedCount} (${totalInstances} instances) totalTris=${(totalTris / 1e6).toFixed(2)}M`
    )
  }

  w.__inspectSplat = (wx?: number, wz?: number) => {
    const deps = getDeps()
    if (wx === undefined || wz === undefined) {
      const p = deps.getPlayerPos()
      if (!p) {
        console.log(
          '[InspectSplat] no player position — call __inspectSplat(x, z)'
        )
        return
      }
      wx = p.x
      wz = p.z
    }
    const TILE_DIM = TERRAIN_TILE_SIZE
    const { tileX, tileZ, cellX: cx, cellZ: cz } = worldToTileCell(wx, wz)
    console.log(
      `[InspectSplat] world=(${wx.toFixed(2)}, ${wz.toFixed(2)}) tile=(${tileX},${tileZ}) cell=(${cx},${cz})`
    )
    const tileCache = new Map<string, Uint8Array | null>()
    const getTile = (tx: number, tz: number) => {
      const key = `${tx},${tz}`
      let data = tileCache.get(key)
      if (data === undefined) {
        data = deps.splatManager.getSplatData(tx, tz) ?? null
        tileCache.set(key, data)
      }
      return data
    }
    const dumpCell = (tx: number, tz: number, ccx: number, ccz: number) => {
      const data = getTile(tx, tz)
      if (!data) return `tile(${tx},${tz}) not loaded`
      const c = readCell(data, ccz * TILE_DIM + ccx)
      return `p=${c.primaryIdx} s=${c.secondaryIdx} b=${c.blend} v=${c.vegMeta}`
    }
    const wrap = (c: number, t: number): [number, number] => {
      if (c < 0) return [c + TILE_DIM, t - 1]
      if (c >= TILE_DIM) return [c - TILE_DIM, t + 1]
      return [c, t]
    }
    for (let dz = 2; dz >= -2; dz--) {
      const row: string[] = []
      for (let dx = -2; dx <= 2; dx++) {
        const [ncx, ntx] = wrap(cx + dx, tileX)
        const [ncz, ntz] = wrap(cz + dz, tileZ)
        const tag = dx === 0 && dz === 0 ? '*' : ' '
        row.push(`${tag}[${dumpCell(ntx, ntz, ncx, ncz)}]`)
      }
      console.log(`dz=${dz >= 0 ? '+' : ''}${dz}: ${row.join(' ')}`)
    }
  }

  return () => {
    for (const key of DEBUG_KEYS) {
      delete w[key]
    }
  }
}
