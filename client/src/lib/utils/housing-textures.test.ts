import { afterEach, describe, expect, it, vi } from 'vitest'
import { SRGBColorSpace, Texture, TextureLoader } from 'three'
import {
  disposeHousingMaterials,
  getGhostHousingMaterial,
  getHousingMaterial,
  initHousingTextures,
} from './housing-textures'
import { DUNGEON_CAVE_THEMES } from './dungeon-cave-themes'

vi.mock('./splatLayerLoader', () => ({
  loadSplatLayer: vi.fn(async () => ({ map: new Texture() })),
}))

afterEach(() => {
  disposeHousingMaterials()
  vi.restoreAllMocks()
})

describe('standalone dungeon textures', () => {
  it.each(DUNGEON_CAVE_THEMES)(
    'updates a retained $id ghost after loading',
    async (theme) => {
      vi.spyOn(TextureLoader.prototype, 'loadAsync').mockImplementation(
        async () => new Texture()
      )
      const idx = theme.wallTexture
      const ghost = getGhostHousingMaterial(idx)
      const opacity = ghost.opacity
      await initHousingTextures()
      const base = getHousingMaterial(idx)
      expect(base.map!.colorSpace).toBe(SRGBColorSpace)
      expect(getGhostHousingMaterial(idx)).toBe(ghost)
      expect(ghost.map).toBe(base.map)
      expect(ghost.bumpMap).toBe(base.bumpMap)
      expect(ghost.bumpScale).toBe(base.bumpScale)
      expect(ghost.opacity).toBe(opacity)
      expect(ghost.vertexColors).toBe(theme.id === 'masonry')
      expect(ghost.opacity).toBe(theme.id === 'masonry' ? 0.22 : 0.5)
      expect(ghost.transparent).toBe(true)
      expect(ghost.depthWrite).toBe(false)
    }
  )

  it('releases owned image maps while retaining shared GLB maps', async () => {
    vi.spyOn(TextureLoader.prototype, 'loadAsync').mockImplementation(
      async () => new Texture()
    )
    await initHousingTextures()
    const cave = getHousingMaterial(DUNGEON_CAVE_THEMES[0].wallTexture)
    const disposeCave = vi.spyOn(cave.map!, 'dispose')
    const disposeShared = vi.spyOn(getHousingMaterial(0).map!, 'dispose')
    disposeHousingMaterials()
    expect(disposeCave).toHaveBeenCalledOnce()
    expect(disposeShared).not.toHaveBeenCalled()
  })
})
