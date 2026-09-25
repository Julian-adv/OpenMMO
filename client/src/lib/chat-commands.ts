import { MathUtils } from 'three'
import { get } from 'svelte/store'
import { translate, type MessageKey } from './i18n'
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
  capeEnabled,
  capeCollarBiasOverride,
} from './stores/debugStore'
import { COLLAR_BIAS_LIMIT } from './effects/cape-rig'
import { computeGrassPlacement, regenerateVegMeta } from './utils/grass-data'
import { teleportLocalPlayer } from './utils/teleport'
import { parseTpArgs, resolveTpDestination } from './utils/tp-args'
import { tpDestinations } from './utils/tp-destinations'

import { dungeonManager } from './managers/dungeonManager'
import { chatChannel } from './stores/chatChannelStore'
import { partyRoster } from './stores/partyStore'
import { EMOTE_LIST } from './emote-meta'
import { DEBUG_ANIM_NAMES, emoteRequest } from './stores/emoteStore'

function teleportTo(x: number, y: number, z: number) {
  const wrappedX = teleportLocalPlayer(x, y, z)
  addChatMessage({
    text: translate('command.teleport', {
      x: wrappedX.toFixed(1),
      y: y.toFixed(1),
      z: z.toFixed(1),
    }),
    sender: 'system',
  })
}

/** Command help, permissions, and optional client handler. */
type Command = {
  desc: MessageKey
  admin?: boolean
  /** Client-side handler. Omit it to let the text through to the server. */
  run?: (args: string) => void
}

const COMMANDS: Record<string, Command> = {
  '/help': {
    desc: 'command.help.help',
    run: () => {
      const regular: string[] = []
      const adminOnly: string[] = []
      for (const name of visibleCommandNames()) {
        ;(COMMANDS[name].admin ? adminOnly : regular).push(name)
      }
      const line = (name: string) =>
        addChatMessage({
          text: `${name} — ${translate(COMMANDS[name].desc)}`,
          sender: 'system',
        })

      addChatMessage({ text: translate('command.available'), sender: 'system' })
      for (const name of regular) line(name)

      if (adminOnly.length > 0) {
        addChatMessage({ text: translate('command.admin'), sender: 'system' })
        for (const name of adminOnly) line(name)
      }
    },
  },

  '/who': { desc: 'command.help.who' },
  '/title': {
    desc: 'command.help.title',
  },
  '/escape': { desc: 'command.help.escape' },
  '/w': { desc: 'command.help.w' },
  '/whisper': { desc: 'command.help.whisper' },
  '/r': { desc: 'command.help.r' },
  '/reply': { desc: 'command.help.reply' },
  '/block': { desc: 'command.help.block' },
  '/unblock': { desc: 'command.help.unblock' },
  '/friend': {
    desc: 'command.help.friend',
  },
  '/f': { desc: 'command.help.f' },
  '/party': { desc: 'command.help.party' },
  '/trade': { desc: 'command.help.trade' },
  '/p': {
    desc: 'command.help.p',
    run: (args) => {
      const message = args.trim()
      if (!get(partyRoster)) {
        addChatMessage({
          text: translate('server.partyNotInParty'),
          localization: { code: 'server.partyNotInParty', params: {} },
          sender: 'system',
        })
        return
      }
      chatChannel.set('party')
      if (message) networkManager.sendPartyChat(message)
    },
  },
  '/s': {
    desc: 'command.help.s',
    run: (args) => {
      const message = args.trim()
      chatChannel.set('say')
      if (message) networkManager.sendChatMessage(message)
    },
  },
  // The server resolves songs and synchronizes music with the emote.
  '/play_music': {
    desc: 'command.help.play_music',
  },
  '/play_instrument': {
    desc: 'command.help.play_instrument',
    run: () => networkManager.sendStartInstrument(),
  },
  '/emote': {
    desc: 'command.help.emote',
    run: (args) => {
      const name = args.trim()
      if (name) {
        // The server is the validator; forward like an unhandled command.
        networkManager.sendChatMessage(`/emote ${name}`)
        return
      }
      addChatMessage({
        text: translate('command.emotes', {
          list: EMOTE_LIST.map((e) => e.anim).join(', '),
        }),
        sender: 'system',
      })
      addChatMessage({
        text: translate('command.emoteHint'),
        sender: 'system',
      })
    },
  },
  // Debug clips are resolved locally from animation packs.
  '/anim': {
    desc: 'command.help.anim',
    admin: true,
    run: (args) => {
      const name = args.trim()
      if (!name) {
        addChatMessage({
          text: translate('command.animUsage'),
          sender: 'system',
        })
        return
      }
      DEBUG_ANIM_NAMES.add(name)
      emoteRequest.set(name)
    },
  },
  '/give': {
    desc: 'command.help.give',
    admin: true,
  },
  '/weather': {
    desc: 'command.help.weather',
    admin: true,
  },
  '/spawnmob': {
    desc: 'command.help.spawnmob',
    admin: true,
  },
  '/notice': {
    desc: 'command.help.notice',
    admin: true,
  },
  '/kick': { desc: 'command.help.kick', admin: true },
  '/ban': {
    desc: 'command.help.ban',
    admin: true,
  },
  '/unban': {
    desc: 'command.help.unban',
    admin: true,
  },
  '/mute': {
    desc: 'command.help.mute',
    admin: true,
  },
  '/unmute': { desc: 'command.help.unmute', admin: true },
  '/summon': {
    desc: 'command.help.summon',
    admin: true,
  },
  '/goto': { desc: 'command.help.goto', admin: true },

  '/pos': {
    desc: 'command.help.pos',
    run: () => {
      const player = get(gameStore).currentPlayer
      if (player) {
        const pos = player.position
        const { tileX, tileZ, cellX, cellZ } = worldToTileCell(pos.x, pos.z)
        const deg = MathUtils.radToDeg(player.rotation).toFixed(1)
        addChatMessage({
          text: translate('command.position', {
            x: pos.x.toFixed(1),
            y: pos.y.toFixed(1),
            z: pos.z.toFixed(1),
            tileX,
            tileZ,
            cellX,
            cellZ,
            degrees: deg,
          }),
          sender: 'system',
        })
      } else {
        addChatMessage({
          text: translate('command.positionUnknown'),
          sender: 'system',
        })
      }
    },
  },

  '/tp': {
    desc: 'command.help.tp',
    admin: true,
    run: (args) => {
      const trimmed = args.trim()
      if (!trimmed) {
        addChatMessage({
          text: translate('command.tpList'),
          sender: 'system',
        })
        for (const [i, d] of tpDestinations().entries()) {
          addChatMessage({
            text: `${i + 1}. ${d.name} — ${d.label}`,
            sender: 'system',
          })
        }
        return
      }

      if (!/\s/.test(trimmed)) {
        const dest = resolveTpDestination(trimmed, tpDestinations())
        if (!dest) {
          addChatMessage({
            text: translate('command.tpUnknown', { name: trimmed }),
            sender: 'system',
          })
          return
        }
        teleportTo(dest.x, dest.y, dest.z)
        return
      }

      const parsed = parseTpArgs(trimmed)
      if (!parsed) {
        addChatMessage({
          text: translate('command.tpUsage'),
          sender: 'system',
        })
        return
      }
      teleportTo(parsed.x, parsed.y, parsed.z)
    },
  },

  '/drop': {
    desc: 'command.help.drop',
    admin: true,
    run: (args) => {
      const player = get(gameStore).currentPlayer
      if (!player) {
        addChatMessage({
          text: translate('command.dropUnknown'),
          sender: 'system',
        })
        return
      }

      const itemDefId = args.trim() || 'goblin_sword'
      networkManager.sendDebugDropItem(itemDefId)

      addChatMessage({
        text: translate('command.drop', { item: itemDefId }),
        sender: 'system',
      })
    },
  },

  '/time': {
    desc: 'command.help.time',
    admin: true,
    run: (args) => {
      const match = args.trim().match(/^(\d{1,2})(?::(\d{1,2}))?$/)
      if (!match) {
        addChatMessage({
          text: translate('command.timeUsage'),
          sender: 'system',
        })
        return
      }
      const hour = Math.min(parseInt(match[1], 10), 23)
      const minute = Math.min(match[2] ? parseInt(match[2], 10) : 0, 59)
      networkManager.sendDebugSetTime(hour, minute)
      addChatMessage({
        text: translate('command.time', {
          hour,
          minute: String(minute).padStart(2, '0'),
        }),
        sender: 'system',
      })
    },
  },

  '/dungeon': {
    desc: 'command.help.dungeon',
    admin: true,
    run: (args) => {
      const player = get(gameStore).currentPlayer
      if (!player) {
        addChatMessage({
          text: translate('command.dungeonUnknown'),
          sender: 'system',
        })
        return
      }

      const arg = args.trim()
      if (arg === 'exit') {
        const ent = dungeonManager.entrancePos
        if (ent) {
          networkManager.sendDebugTeleport({ x: ent.x, y: ent.y, z: ent.z })
        }
        dungeonManager.exit()
        addChatMessage({
          text: translate('command.dungeonExited'),
          sender: 'system',
        })
        return
      }

      if (arg === 'resetprops' || arg === 'reset-props') {
        const entranceId = dungeonManager.dungeonId
        if (!entranceId) {
          addChatMessage({
            text: translate('command.dungeonInactive'),
            sender: 'system',
          })
          return
        }
        networkManager.sendDebugResetDungeonProps(entranceId)
        addChatMessage({
          text: translate('command.dungeonReset'),
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
        addChatMessage({
          text: translate('command.dungeonMissing'),
          sender: 'system',
        })
        return
      }
      const target = dungeonManager.cellCenter(
        depth,
        dungeonManager.shaftExitCell(layout.upShaft)
      )
      dungeonManager.setDepth(depth)
      networkManager.sendDebugTeleport(target)
      addChatMessage({
        text: translate('command.dungeonDepth', {
          depth,
          total,
          rooms: layout.rooms.length,
          spawns: layout.spawns.length,
        }),
        sender: 'system',
      })
    },
  },

  '/wireframe': {
    desc: 'command.help.wireframe',
    run: () => {
      const next = !get(riverWireframeVisible)
      riverWireframeVisible.set(next)
      addChatMessage({
        text: translate('command.riverWireframe', {
          state: translate(next ? 'command.on' : 'command.off'),
        }),
        sender: 'system',
      })
    },
  },

  '/shore_wave': {
    desc: 'command.help.shore_wave',
    run: () => {
      const next = !get(shoreWaveDebugVisible)
      shoreWaveDebugVisible.set(next)
      addChatMessage({
        text: translate('command.shoreWave', {
          state: translate(next ? 'command.on' : 'command.off'),
        }),
        sender: 'system',
      })
    },
  },

  '/cape': {
    desc: 'command.help.cape',
    run: () => {
      const next = !get(capeEnabled)
      capeEnabled.set(next)
      addChatMessage({
        text: translate('command.cape', {
          state: translate(next ? 'command.on' : 'command.off'),
        }),
        sender: 'system',
      })
    },
  },

  '/cape_depth': {
    desc: 'command.help.cape_depth',
    run: (args) => {
      const trimmed = args.trim()
      if (!trimmed) {
        const override = get(capeCollarBiasOverride)
        addChatMessage({
          text: translate('command.capeDepthHint', {
            depth:
              override === null
                ? translate('command.capeDefault')
                : `${override.toFixed(3)} m`,
          }),
          sender: 'system',
        })
        return
      }
      if (trimmed === 'auto' || trimmed === 'reset') {
        capeCollarBiasOverride.set(null)
        addChatMessage({
          text: translate('command.capeAuto'),
          sender: 'system',
        })
        return
      }
      const value = Number(trimmed)
      if (!Number.isFinite(value)) {
        addChatMessage({
          text: translate('command.capeUsage'),
          sender: 'system',
        })
        return
      }
      const clamped = MathUtils.clamp(
        value,
        -COLLAR_BIAS_LIMIT,
        COLLAR_BIAS_LIMIT
      )
      capeCollarBiasOverride.set(clamped)
      addChatMessage({
        text: translate('command.capeDepth', { depth: clamped.toFixed(3) }),
        sender: 'system',
      })
    },
  },

  '/passability': {
    desc: 'command.help.passability',
    run: () => {
      const next = !get(passabilityDebugVisible)
      passabilityDebugVisible.set(next)
      addChatMessage({
        text: translate('command.passability', {
          state: translate(next ? 'command.on' : 'command.off'),
        }),
        sender: 'system',
      })
    },
  },

  '/regrow': {
    desc: 'command.help.regrow',
    admin: true,
    run: () => {
      const player = get(gameStore).currentPlayer
      if (!player) {
        addChatMessage({
          text: translate('command.regrowUnknown'),
          sender: 'system',
        })
        return
      }

      const hMgr = get(editorHeightManager)
      const sMgr = get(editorSplatManager)
      const gMgr = get(editorGrassDataManager)
      if (!hMgr || !sMgr || !gMgr) {
        addChatMessage({
          text: translate('command.regrowNotReady'),
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
          text: translate('command.regrowNoSplat', { x: tileX, z: tileZ }),
          sender: 'system',
        })
        return
      }

      addChatMessage({
        text: translate('command.regrowStart', { x: tileX, z: tileZ }),
        sender: 'system',
      })

      regenerateVegMeta(splatData, tileX, tileZ)
      // Refresh GPU texture + mark tile dirty for the debounced save.
      sMgr.setSplatmap(tileX, tileZ, splatData)
      sMgr.markDirty(tileX, tileZ)
      sMgr.saveAllDirty().catch((err) => {
        addChatMessage({
          text: translate('command.regrowSplatFailed', { error: String(err) }),
          sender: 'system',
        })
      })

      const data = computeGrassPlacement(tileX, tileZ, splatData, hMgr)
      gMgr.saveGrassData(tileX, tileZ, data).then(
        () => {
          addChatMessage({
            text: translate('command.regrowDone', {
              short: data.shortCount,
              tall: data.tallCount,
              flower: data.flowerCount,
            }),
            sender: 'system',
          })
        },
        (err) => {
          addChatMessage({
            text: translate('command.regrowGrassFailed', {
              error: String(err),
            }),
            sender: 'system',
          })
        }
      )
    },
  },
}

/** `/help` first so a bare `/` completes to it. */
const commandNames = [
  '/help',
  ...Object.keys(COMMANDS)
    .filter((n) => n !== '/help')
    .sort(),
]

/** Command names for autocomplete; hides admin commands from non-admins. */
export function visibleCommandNames(): string[] {
  if (get(isAdminUser)) return commandNames
  return commandNames.filter((n) => !COMMANDS[n].admin)
}

export function handleCommand(input: string): boolean {
  const spaceIndex = input.indexOf(' ')
  const name = spaceIndex === -1 ? input : input.slice(0, spaceIndex)
  const args = spaceIndex === -1 ? '' : input.slice(spaceIndex + 1)
  const command = COMMANDS[name]
  if (!command?.run) return false
  if (command.admin && !get(isAdminUser)) {
    addChatMessage({
      text: translate('command.adminOnly', { name }),
      sender: 'system',
    })
    return true
  }
  command.run(args)
  return true
}
