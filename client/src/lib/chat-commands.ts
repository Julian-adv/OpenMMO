import { MathUtils } from 'three'
import { get } from 'svelte/store'
import { gameStore, addChatMessage, isAdminUser } from './stores/gameStore'
import { worldToTileCell } from './components/game-scene/terrain-utils'
import { networkManager } from './network/socket'
import {
  editorHeightManager,
  editorSplatManager,
  editorGrassDataManager,
} from './stores/editorStore'
import {
  riverWireframeVisible,
  shoreWaveDebugVisible,
  passabilityDebugVisible,
} from './stores/debugStore'
import { computeGrassPlacement, regenerateVegMeta } from './utils/grass-data'
import { dungeonManager } from './managers/dungeonManager'

type CommandHandler = (args: string) => void

const ADMIN_COMMANDS = new Set(['/drop', '/time', '/dungeon'])

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

  '/drop': (args) => {
    const player = get(gameStore).currentPlayer
    if (!player) {
      addChatMessage({
        text: 'Drop: player position unknown',
        sender: 'system',
      })
      return
    }

    const itemDefId = args.trim() || 'goblin_sword'
    networkManager.sendDebugDropItem(itemDefId)

    addChatMessage({
      text: `Drop: requested ${itemDefId} near 1m ahead`,
      sender: 'system',
    })
  },

  '/time': (args) => {
    const match = args.trim().match(/^(\d{1,2})(?::(\d{1,2}))?$/)
    if (!match) {
      addChatMessage({
        text: 'Usage: /time HH[:MM] — jump the game clock forward to that time (e.g. /time 9:30)',
        sender: 'system',
      })
      return
    }
    const hour = Math.min(parseInt(match[1], 10), 23)
    const minute = Math.min(match[2] ? parseInt(match[2], 10) : 0, 59)
    networkManager.sendDebugSetTime(hour, minute)
    addChatMessage({
      text: `Time: requested jump to ${hour}:${String(minute).padStart(2, '0')}`,
      sender: 'system',
    })
  },

  '/dungeon': (args) => {
    const player = get(gameStore).currentPlayer
    if (!player) {
      addChatMessage({ text: 'Dungeon: player unknown', sender: 'system' })
      return
    }

    const arg = args.trim()
    if (arg === 'exit') {
      const ent = dungeonManager.entrancePos
      if (ent) {
        networkManager.sendDebugTeleport({ x: ent.x, y: ent.y, z: ent.z })
      }
      dungeonManager.exit()
      addChatMessage({ text: 'Dungeon: exited to surface', sender: 'system' })
      return
    }

    if (arg === 'resetprops' || arg === 'reset-props') {
      const entranceId = dungeonManager.dungeonId
      if (!entranceId) {
        addChatMessage({
          text: 'Dungeon props: no active dungeon',
          sender: 'system',
        })
        return
      }
      networkManager.sendDebugResetDungeonProps(entranceId)
      addChatMessage({
        text: 'Dungeon props: reset requested',
        sender: 'system',
      })
      return
    }

    const requested = Math.max(1, parseInt(arg || '1', 10) || 1)
    if (!dungeonManager.active) {
      // Debug dungeon anchored at the player's current position.
      dungeonManager.enter('debug', {
        x: player.position.x,
        y: player.position.y,
        z: player.position.z,
      })
    }
    const total = dungeonManager.floors.length
    const depth = Math.min(requested, total)
    const layout = dungeonManager.layoutAt(depth)
    if (!layout) {
      addChatMessage({ text: 'Dungeon: layout missing', sender: 'system' })
      return
    }
    const target = dungeonManager.cellCenter(
      depth,
      dungeonManager.shaftExitCell(layout.upShaft)
    )
    dungeonManager.setDepth(depth)
    networkManager.sendDebugTeleport(target)
    addChatMessage({
      text: `Dungeon: depth ${depth}/${total} (rooms=${layout.rooms.length}, spawns=${layout.spawns.length})`,
      sender: 'system',
    })
  },

  '/wireframe': () => {
    const next = !get(riverWireframeVisible)
    riverWireframeVisible.set(next)
    addChatMessage({
      text: `River wireframe: ${next ? 'on' : 'off'}`,
      sender: 'system',
    })
  },

  '/shore_wave': () => {
    const next = !get(shoreWaveDebugVisible)
    shoreWaveDebugVisible.set(next)
    addChatMessage({
      text: `Shore wave debug: ${next ? 'on' : 'off'}`,
      sender: 'system',
    })
  },

  '/passability': () => {
    const next = !get(passabilityDebugVisible)
    passabilityDebugVisible.set(next)
    addChatMessage({
      text: `Passability debug: ${next ? 'on' : 'off'} (walls red, furniture orange)`,
      sender: 'system',
    })
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

/** Handled by the server (`game_state::chat`); listed only for autocomplete. */
const SERVER_COMMANDS = ['/escape', '/who']

const commandNames = [...Object.keys(commands), ...SERVER_COMMANDS].sort()

/** Command names for autocomplete; hides admin commands from non-admins. */
export function visibleCommandNames(): string[] {
  if (get(isAdminUser)) return commandNames
  return commandNames.filter((n) => !ADMIN_COMMANDS.has(n))
}

export function handleCommand(input: string): boolean {
  const spaceIndex = input.indexOf(' ')
  const name = spaceIndex === -1 ? input : input.slice(0, spaceIndex)
  const args = spaceIndex === -1 ? '' : input.slice(spaceIndex + 1)
  const handler = commands[name]
  if (!handler) return false
  if (ADMIN_COMMANDS.has(name) && !get(isAdminUser)) {
    addChatMessage({ text: `${name}: admin only`, sender: 'system' })
    return true
  }
  handler(args)
  return true
}
