import { MathUtils } from 'three'
import { get } from 'svelte/store'
import { gameStore, addChatMessage } from './stores/gameStore'
import { worldToTileCell } from './components/game-scene/terrain-utils'
import {
  editorHeightManager,
  editorSplatManager,
  editorGrassDataManager,
} from './stores/editorStore'
import { computeGrassPlacement, regenerateVegMeta } from './utils/grass-data'

type CommandHandler = (args: string) => void

const commands: Record<string, CommandHandler> = {
  '/pos': () => {
    const player = get(gameStore).currentPlayer
    if (player) {
      const pos = player.position
      const { tileX, tileZ, cellX, cellZ } = worldToTileCell(pos.x, pos.z)
      const deg = MathUtils.radToDeg(player.rotation).toFixed(1)
      addChatMessage({
        text: `Position: world(${pos.x.toFixed(1)}, ${pos.y.toFixed(1)}, ${pos.z.toFixed(1)}) tile(${tileX}, ${tileZ}) cell(${cellX}, ${cellZ}) rot(${deg}°)`,
        sender: 'system',
      })
    } else {
      addChatMessage({ text: 'Position: unknown', sender: 'system' })
    }
  },

  '/regrow': () => {
    const player = get(gameStore).currentPlayer
    if (!player) {
      addChatMessage({
        text: 'Regrow: player position unknown',
        sender: 'system',
      })
      return
    }

    const hMgr = get(editorHeightManager)
    const sMgr = get(editorSplatManager)
    const gMgr = get(editorGrassDataManager)
    if (!hMgr || !sMgr || !gMgr) {
      addChatMessage({
        text: 'Regrow: terrain managers not ready',
        sender: 'system',
      })
      return
    }

    const { tileX, tileZ } = worldToTileCell(
      player.position.x,
      player.position.z
    )
    const splatData = sMgr.getSplatData(tileX, tileZ)
    if (!splatData) {
      addChatMessage({
        text: `Regrow: no splatmap for tile(${tileX}, ${tileZ})`,
        sender: 'system',
      })
      return
    }

    addChatMessage({
      text: `Regrow: regenerating grass for tile(${tileX}, ${tileZ})...`,
      sender: 'system',
    })

    regenerateVegMeta(splatData, tileX, tileZ)
    // Refresh GPU texture + mark tile dirty for the debounced save.
    sMgr.setSplatmap(tileX, tileZ, splatData)
    sMgr.markDirty(tileX, tileZ)
    sMgr.saveAllDirty().catch((err) => {
      addChatMessage({
        text: `Regrow: splatmap save failed — ${err}`,
        sender: 'system',
      })
    })

    const data = computeGrassPlacement(tileX, tileZ, splatData, hMgr)
    gMgr.saveGrassData(tileX, tileZ, data).then(
      () => {
        addChatMessage({
          text: `Regrow: done — short=${data.shortCount} tall=${data.tallCount} flower=${data.flowerCount}`,
          sender: 'system',
        })
      },
      (err) => {
        addChatMessage({
          text: `Regrow: grass save failed — ${err}`,
          sender: 'system',
        })
      }
    )
  },
}

export function handleCommand(input: string): boolean {
  const spaceIndex = input.indexOf(' ')
  const name = spaceIndex === -1 ? input : input.slice(0, spaceIndex)
  const args = spaceIndex === -1 ? '' : input.slice(spaceIndex + 1)
  const handler = commands[name]
  if (handler) {
    handler(args)
    return true
  }
  return false
}
