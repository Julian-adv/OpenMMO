import { manaState } from '../stores/manaStore'
import { translate, type MessageKey } from '../i18n'
import {
  playTeleportEffect,
  finishTeleportArrival,
  resetTeleportEffects,
  deferRemoteTeleportUpdate,
  cancelLocalTeleportRequest,
} from '../stores/teleportEffectStore'
import {
  inspectionResult,
  type InspectionResult,
} from '../stores/inspectionStore'
import { get } from 'svelte/store'
import { attackLog, daggerSkippedLog } from './combatLog'
import {
  acknowledgeDaggerSkill,
  clearDaggerCast,
  daggerSkillState,
  playDaggerSkill,
} from '../stores/daggerSkillStore'
import {
  applyAbilityCooldowns,
  abilityPending,
  activeBuffs,
  updateBowMark,
  timerSnapshot,
  queueAbilityEffect,
  type AbilityEffectEvent,
} from '../stores/abilityStore'
import {
  DOUBLE_SLASH,
  GUARDIAN_WARD,
  abilityRequirementsNotMet,
  abilityEquipmentNotMet,
  abilityDisplayName,
  type AbilityTimer,
} from '../data/abilities'
import {
  landAccount,
  landAccountError,
  landTransferPending,
} from '../stores/landAccountStore'
import {
  gameStore,
  updatePlayer,
  addChatMessage,
  reportSkillFailure,
  addCombatMessage,
  addChatBubble,
  resetGameStore,
  isAdminUser,
  serverNotice,
} from '../stores/gameStore'
import type { GameState, LocalPlayer, RemotePlayer } from '../stores/gameStore'
import { playerHealthDisplay } from '../stores/playerHealthDisplay'
import { Vector3 } from 'three'
import { remotePlayerManager } from '../managers/remotePlayerManager'
import { FishingAnimationName } from '../types/animations'
import {
  cancelPendingFishingSounds,
  playFishingSound,
  playDungeonSound,
  playPlayerDeathSound,
  playPlayerHurtSound,
  playPropSound,
  playSwordMissSound,
} from '../managers/sfxManager'
import { FISHING_CAST_SWING_DELAY_MS } from '../data/combatTiming'
import { monsterManager } from '../managers/monsterManager'
import { housingManager } from '../managers/housingManager'
import { bridgeManager } from '../managers/bridgeManager'
import { objectManager } from '../managers/objectManager'
import { groundItemManager } from '../managers/groundItemManager'
import { dungeonManager } from '../managers/dungeonManager'
import { queueXpArrival, releaseXpArrival } from '../managers/xpArrival'
import {
  setInventory,
  playerGold,
  playerEffectiveStats,
} from '../stores/inventoryStore'
import { queueEnchantSuccess } from '../stores/enchantSuccessStore'
import { capeDyeDialog } from '../stores/capeDyeStore'
import {
  applyFenceVisibility,
  fencePending,
  fenceError,
  resetFences,
  stopFenceMode,
} from '../stores/fenceStore'
import {
  openLandscapingMode,
  landscapingMode,
  landscapingPending,
  landscapingError,
  selectLandscapingTool,
} from '../stores/landscapingStore'
import type { LandscapingTile } from '../terrain/landscaping'
import {
  applyEstateChestVisibility,
  estateChestError,
  estateChestPending,
  openEstateChest,
  resetEstateStorage,
  stopEstateChestMode,
} from '../stores/estateStorageStore'
import { inventoryVisible } from '../stores/debugStore'
import {
  landClaimDialog,
  applyLandClaimPreview,
} from '../stores/landClaimStore'
import {
  applyHousePlacementResult,
  applyHouseDemolitionResult,
  openHousePlacement,
  resetHousePlacement,
} from '../stores/housePlacementStore'
import { capeTextureDialog } from '../stores/capeTextureStore'
import { setCapeUploadToken } from '../utils/networkUtils'
import { hungerState, grilling, type HungerBand } from '../stores/hungerStore'
import { activeDebuffs, type ActiveDebuff } from '../stores/debuffStore'
import { debuffPresentation } from '../data/debuffPresentation'
import { campfireManager } from '../managers/campfireManager'
import { stallManager } from '../managers/stallManager'
import { tipHatManager } from '../managers/tipHatManager'
import { closeStallPanel, openStall } from '../stores/stallStore'
import { mealManager } from '../managers/mealManager'
import { catchMessage } from './fishingMessages'
import { earnedTitles } from '../stores/titleStore'
import { titleNameNow } from '../data/titleDefs'
import { skillsStore } from '../stores/skillsStore'
import {
  myFishing,
  applyFightUpdate,
  upsertBobber,
  markBobberBite,
  updateBobberFight,
  landFishingCatch,
  removeBobber,
} from '../stores/fishingStore'
import { getItemDef, itemDisplayName } from '../data/itemDefs'
import { getMonsterDef } from '../data/monsterDefs'
import { getMaterialMissSoundUrl } from '../data/materialImpactSounds'
import {
  shopSession,
  applyDealUpdate,
  setMerchantDeals,
  wasShopRequested,
  pendingTradeOffer,
  type BuybackEntry,
} from '../stores/tradeStore'
import {
  partyRoster,
  applyPartyPositions,
  applyPartyVitals,
  resetPartyPositions,
  resetPartyStores,
  pendingPartyInvites,
  pendingPartySummons,
  SUMMON_TTL_MS,
  MAX_PENDING_PARTY_INVITES,
  type PartyMemberEntry,
  type PartyMemberPositionEntry,
  type PartyMemberVitalsEntry,
} from '../stores/partyStore'
import {
  dismissTradeRequest,
  enqueueTradeRequest,
  playerTrade,
  playerTradeError,
} from '../stores/playerTradeStore'
import {
  applyFriendList,
  applyFriendsOnline,
  friendList,
  friendOnlineNoticeEnabled,
  pendingFriendRequests,
  resetFriendStores,
  MAX_PENDING_FRIEND_REQUESTS,
} from '../stores/friendStore'
import { enqueueConsent } from '../stores/consentQueue'
import {
  editorHeightManager,
  editorTreeDataManager,
  editorGrassDataManager,
  editorSplatManager,
} from '../stores/editorStore'
import { discoveredDungeonIds } from '../stores/dungeonStore'
import { requestCameraReset } from '../stores/cameraStore'
import { setServerGameTime } from '../stores/timeStore'
import { setWeather } from '../stores/weatherStore'
import { combatController } from '../managers/combatController'
import { playerVisualFloorLevel } from '../stores/housingStore'
import { currentDungeonDepth } from '../stores/dungeonStore'
import {
  startMusicPerformance,
  stopMusicPerformance,
  fadeOutMusicPerformance,
  applyInteractionChange,
} from '../managers/musicPerformance'
import { refreshBardZone } from '../managers/bardZone'
import { holdLiveInstrumentQuiet } from '../managers/bgmManager'
import {
  emoteRequest,
  emotePanelVisible,
  emoteStopRequest,
  isEmoteAnim,
  MUSIC_EMOTE_ANIM,
  SLASH_EMOTE_ANIMS,
} from '../stores/emoteStore'
import { respawnPoseRequest } from '../stores/respawnPoseStore'
import { syncOwnFloor } from './ownFloor'
import {
  closeInstrumentPanel,
  openInstrumentPanel,
} from '../stores/instrumentStore'
import {
  instrumentDistanceGain,
  playInstrumentNote,
  stopInstrumentPerformer,
} from '../managers/instrumentAudio'
import { shortestWrappedDeltaX } from '../terrain/world-wrap'
import { whisperChatEntry, partyChatEntry } from '../chat-format'
import {
  fishing_cast_ms,
  fishing_trophy_min_tension,
} from '../wasm/onlinerpg_shared'
import type { NetworkEvent } from './networkEvents'
import type {
  AccountCharacter,
  AuthSuccessPayload,
  CharacterAttributes,
  CharacterRollResult,
  ServerGroundItem,
  PositionCorrection,
  MountRecovery,
  MovePath,
  MoveProgress,
  ServerMonster,
  ServerPlayer,
  CharacterClass,
} from './networkTypes'

/** A recited verse stays up until the next one lands (the bard sends one every ~9s). */
const RECITAL_BUBBLE_MS = 12000

// A fatal blow arrives twice: as MonsterAttackedPlayer, which lines the cry up
// with the impact frame, and again as PlayerDead. First claim wins so the
// scream never doubles; deaths with no blow behind them (debuff ticks) still
// cry on PlayerDead alone.
const DEATH_CRY_WINDOW_MS = 2000
const deathCriedAt = new Map<string, number>()

function claimPlayerDeath(playerId: string) {
  const now = performance.now()
  for (const [id, at] of deathCriedAt) {
    if (now - at >= DEATH_CRY_WINDOW_MS) deathCriedAt.delete(id)
  }
  if (deathCriedAt.has(playerId)) return false
  deathCriedAt.set(playerId, now)
  return true
}

function mapBuyback(
  entries:
    | {
        entry_id: number
        item_def_id: string
        enchant: number
        price: number
      }[]
    | undefined
): BuybackEntry[] {
  return (entries ?? []).map((e) => ({
    entryId: e.entry_id,
    itemDefId: e.item_def_id,
    enchant: e.enchant,
    price: Number(e.price),
  }))
}

function toLocalPlayer(sp: ServerPlayer): LocalPlayer {
  return {
    ...sp,
    position: new Vector3(sp.position.x, sp.position.y, sp.position.z),
    rotation: sp.rotation ?? 0,
    maxHealth: sp.max_health,
    characterClass: sp.class,
    gender: sp.gender,
    radianceOn: sp.radiance_on ?? false,
  }
}

function toRemotePlayer(sp: ServerPlayer): RemotePlayer {
  return {
    id: sp.id,
    name: sp.name,
    level: sp.level,
    health: sp.health,
    maxHealth: sp.max_health,
    characterClass: sp.class,
    gender: sp.gender,
    mount: sp.mount ?? null,
    torchOn: sp.torch_on,
    radianceOn: sp.radiance_on ?? false,
    wet: sp.wet ?? false,
    title: sp.title ?? null,
    mainHand: sp.main_hand ?? null,
    back: sp.back ?? null,
    backColor: sp.back_color ?? null,
    backTexture: sp.back_texture ?? null,
    floorLevel: sp.floor_level ?? 0,
    isOfficialNpc: sp.is_official_npc ?? false,
  }
}

function emitCurrentPlayerDamageInfo(
  playerId: number,
  damage: number,
  hit: boolean,
  currentHealth: number,
  delayMs: number
) {
  const applyImpact = playerHealthDisplay.prepareImpact(
    playerId,
    hit,
    currentHealth
  )
  const emit = () => {
    const state = get(gameStore)
    if (state.currentPlayer?.id !== playerId || !applyImpact()) return

    updatePlayer(playerId, {
      lastDamageInfo: {
        damage,
        hit,
        trigger: (state.currentPlayer.lastDamageInfo?.trigger ?? 0) + 1,
      },
    })
  }

  if (delayMs > 0) {
    globalThis.setTimeout(emit, delayMs)
  } else {
    emit()
  }
}

/** Bump the flinch counter at the monster's impact frame, like the hurt cry.
 *  Only the change is read. Remotes bump the manager's per-player map, which
 *  saves the delayed store republish this would otherwise cost per blow. */
function emitPlayerHit(
  playerId: number,
  isCurrentPlayer: boolean,
  delayMs: number
) {
  const bump = () => {
    if (!isCurrentPlayer) {
      remotePlayerManager.handleHit(playerId)
      return
    }
    const player = get(gameStore).currentPlayer
    if (player?.id === playerId) {
      updatePlayer(playerId, { hitCounter: (player.hitCounter ?? 0) + 1 })
    }
  }

  if (delayMs > 0) {
    globalThis.setTimeout(bump, delayMs)
  } else {
    bump()
  }
}

const remoteEstateInteractions = new Map<
  number,
  { objectType: string; objectId: number | null | undefined }
>()

/** Resolve the remote player's furniture pose. */
async function applyObjectInteraction(
  playerId: number,
  objectType: string,
  wx: number,
  wz: number,
  objectId?: number | null
) {
  const estateInteraction = getEstateStorageDef(objectType)
    ? { objectType, objectId }
    : null
  if (estateInteraction)
    remoteEstateInteractions.set(playerId, estateInteraction)
  else remoteEstateInteractions.delete(playerId)
  if (objectType === 'pickup' || isEmoteAnim(objectType)) {
    remotePlayerManager.handleInteraction(playerId, objectType, 0)
    return
  }

  const { anim, interactOffset, placement, rotation } =
    await objectManager.resolvePose(objectType, wx, wz, objectId)
  if (
    estateInteraction &&
    (remoteEstateInteractions.get(playerId) !== estateInteraction || !placement)
  )
    return
  const pos = placement
    ? { x: placement.x, y: placement.y, z: placement.z }
    : undefined
  remotePlayerManager.handleInteraction(
    playerId,
    anim,
    interactOffset?.y ?? 0,
    pos,
    rotation
  )
}

/** A player's spoken line into the chat log, under their name. */
function logSpokenLine(playerId: number, text: string) {
  const state = get(gameStore)
  const isLocal = state.currentPlayer?.id === playerId
  const speaker = isLocal
    ? state.currentPlayer
    : state.otherPlayers.get(playerId)
  addChatMessage({
    text,
    sender: isLocal ? 'local' : 'remote',
    name: speaker?.name ?? translate('chat.unknown'),
  })
}

/** Spawn a remote player's visual, apply any object interaction, and store it in game state. */
function addRemotePlayerToState(state: GameState, sp: ServerPlayer) {
  remotePlayerManager.initPlayer(sp.id, sp.position, sp.rotation)
  if (sp.object_type) {
    applyObjectInteraction(
      sp.id,
      sp.object_type,
      sp.position.x,
      sp.position.z,
      sp.object_id
    )
  }
  state.otherPlayers.set(sp.id, toRemotePlayer(sp))
  refreshBardZone(state.otherPlayers)
}

/** Remove a remote player's visual and store entry. */
function removeRemotePlayerFromState(state: GameState, playerId: number) {
  remoteEstateInteractions.delete(playerId)
  remotePlayerManager.removePlayer(playerId)
  state.otherPlayers.delete(playerId)
  refreshBardZone(state.otherPlayers)
  // A leaving player's FishingEnded may never arrive; drop their bobber.
  removeBobber(playerId)
}

export type MessageEvents = {
  authSuccess: NetworkEvent<(payload: AuthSuccessPayload) => void>
  authError: NetworkEvent<(message: string) => void>
  joinSuccess: NetworkEvent<() => void>
  characterCreated: NetworkEvent<(character: AccountCharacter) => void>
  characterStatsRolled: NetworkEvent<(result: CharacterRollResult) => void>
  characterDeleted: NetworkEvent<(characterId: number) => void>
  characterRenameRequired: NetworkEvent<(characterId: number) => void>
  characterRenamed: NetworkEvent<
    (payload: { characterId: number; name: string }) => void
  >
  characterError: NetworkEvent<(message: string) => void>
  kicked: NetworkEvent<(reason: string) => void>
  playerRespawned: NetworkEvent<(playerId: number) => void>
  interactionRejected: NetworkEvent<(reason: string) => void>
  movePath: NetworkEvent<(path: MovePath) => void>
  moveProgress: NetworkEvent<(progress: MoveProgress) => void>
  mountRecovery: NetworkEvent<(update: MountRecovery) => void>
  positionCorrected: NetworkEvent<(c: PositionCorrection) => void>
}

function isSelfPlayer(playerId: number): boolean {
  return get(gameStore).currentPlayer?.id === playerId
}

const instrumentNoteTimers = new Map<
  number,
  Set<ReturnType<typeof globalThis.setTimeout>>
>()

function clearInstrumentNoteTimers(playerId: number) {
  const timers = instrumentNoteTimers.get(playerId)
  if (!timers) return
  for (const timer of timers) globalThis.clearTimeout(timer)
  instrumentNoteTimers.delete(playerId)
}

function stopPlayerInstrument(playerId: number) {
  clearInstrumentNoteTimers(playerId)
  stopInstrumentPerformer(playerId)
  if (isSelfPlayer(playerId)) closeInstrumentPanel()
}

function localFloorLevel(): number {
  const depth = get(currentDungeonDepth)
  return depth >= 1 ? -depth : get(playerVisualFloorLevel)
}

function playRemoteInstrumentNotes(
  playerId: number,
  position: { x: number; y: number; z: number },
  floorLevel: number,
  events: { note: number; offset_ms: number }[]
) {
  if (isSelfPlayer(playerId) || !position || !Array.isArray(events)) return

  const play = (note: number) => {
    const listener = get(gameStore).currentPlayer
    if (!listener || floorLevel !== localFloorLevel()) return
    const dx = shortestWrappedDeltaX(listener.position.x, position.x)
    const dz = position.z - listener.position.z
    const gain = instrumentDistanceGain(Math.hypot(dx, dz))
    if (gain > 0) holdLiveInstrumentQuiet()
    playInstrumentNote(note, playerId, gain)
  }

  let timers = instrumentNoteTimers.get(playerId)
  if (!timers) {
    timers = new Set()
    instrumentNoteTimers.set(playerId, timers)
  }

  for (const event of events) {
    if (!Number.isInteger(event.note) || !Number.isFinite(event.offset_ms)) {
      continue
    }
    const delay = Math.max(0, Math.min(1000, event.offset_ms))
    if (delay === 0) {
      play(event.note)
      continue
    }
    const timer = globalThis.setTimeout(() => {
      timers?.delete(timer)
      if (timers?.size === 0) instrumentNoteTimers.delete(playerId)
      play(event.note)
    }, delay)
    timers.add(timer)
  }
}

/// Who did it, for a chat line: "You" for us, their name for anyone else.
function actorName(playerId: number): string {
  const state = get(gameStore)
  if (state.currentPlayer?.id === playerId) return translate('chat.you')
  return state.otherPlayers.get(playerId)?.name ?? translate('chat.someone')
}

/// One chat line for a ground item changing hands. Silent unless a player
/// did it (actorId set) and the item is known.
function announceGroundItem(
  actorId: number | null | undefined,
  itemDefId: string | undefined,
  action: 'droppedItem' | 'pickedUpItem',
  quantity = 1
) {
  if (actorId == null || !itemDefId) return
  const name = itemDisplayName(itemDefId)
  const amount = quantity > 1 ? ` x${quantity}` : ''
  addChatMessage({
    text: translate(`system.${action}`, {
      name: actorName(actorId),
      item: name,
      amount,
    }),
    sender: 'system',
  })
}

import { worldView, type WorldUpdate } from './worldView'
import {
  selectedEstateFurniture,
  startEstateFurniturePlacement,
  applyEstateFurnitureEditResult,
} from '../stores/estateFurniturePlacementStore'
import { getEstateStorageDef } from '../data/estateFurnitureDefs'
import type { EstateChest } from './networkTypes'
import { TerrainSnapshots, type TerrainSnapshot } from './terrainSnapshots'
import {
  furniturePurchasePending,
  furnitureShopError,
  clearFurnitureBasket,
} from '../stores/furnitureShopStore'
import { getTerrainApiUrl } from '../utils/networkUtils'

const terrainSnapshots = new Map<string, TerrainSnapshot>()
const terrainDownloads = new TerrainSnapshots(
  getTerrainApiUrl,
  (tile) => {
    terrainSnapshots.set(`${tile.tile_x},${tile.tile_z}`, tile)
    applyTerrainSnapshots([tile])
  },
  () => resyncWorld()
)
export function resetTerrainDownloads() {
  terrainDownloads.reset()
  terrainSnapshots.clear()
  worldView.pendingTerrain.clear()
}
let requestResync = () => {}
let resyncTimer: ReturnType<typeof setTimeout> | undefined
let lastCorrection = -Infinity
function scheduleResync() {
  if (resyncTimer !== undefined) return
  resyncTimer = setTimeout(() => {
    resyncTimer = undefined
    if (!worldView.synchronized) {
      requestResync()
      scheduleResync()
    }
  }, 1000)
}
function resyncWorld() {
  worldView.synchronized = false
  requestResync()
  scheduleResync()
}
function applyTerrainSnapshots(
  tiles: Iterable<TerrainSnapshot> = terrainSnapshots.values()
) {
  const heights = get(editorHeightManager)
  const trees = get(editorTreeDataManager)
  const grass = get(editorGrassDataManager)
  const splat = get(editorSplatManager)
  if (!heights || !trees || !grass || !splat) return
  try {
    for (const tile of tiles) {
      heights.applySnapshot(tile.tile_x, tile.tile_z, tile.height)
      splat.setSplatmap(tile.tile_x, tile.tile_z, new Uint8Array(tile.splat))
      const mask = tile.cleared
      trees.applyLandscapingMask(tile.tile_x, tile.tile_z, mask)
      grass.applyLandscapingMask(tile.tile_x, tile.tile_z, mask)
      trees.applySnapshot(tile.tile_x, tile.tile_z, tile.trees)
      grass.applySnapshot(tile.tile_x, tile.tile_z, tile.grass)
      worldView.pendingTerrain.delete(`${tile.tile_x},${tile.tile_z}`)
    }
  } catch (error) {
    console.error('Terrain snapshot remains pending', error)
    resyncWorld()
  }
}
editorHeightManager.subscribe(() => applyTerrainSnapshots())
editorTreeDataManager.subscribe(() => applyTerrainSnapshots())
editorGrassDataManager.subscribe(() => applyTerrainSnapshots())
editorSplatManager.subscribe(() => applyTerrainSnapshots())

let pendingHeightTileRefresh: Promise<void> = Promise.resolve()

export function handleServerMessage(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  raw: any,
  events: MessageEvents,
  disconnect: () => void,
  resync: () => void
) {
  // Payloadless variants (GrillStarted, DungeonReset) arrive as a bare name.
  const isBare = typeof raw === 'string'
  const type = isBare ? raw : Object.keys(raw)[0]
  const data = isBare ? undefined : raw[type]

  const remoteId =
    type === 'PlayerAppeared'
      ? data.player.id
      : ['PlayerTeleported', 'PlayerMoved', 'PlayerDisappeared'].includes(type)
        ? data.player_id
        : null
  if (remoteId !== null && remoteId !== get(gameStore).currentPlayer?.id) {
    const { epoch, generation } = worldView
    if (
      deferRemoteTeleportUpdate(remoteId, () => {
        if (worldView.epoch === epoch && worldView.generation === generation)
          handleServerMessage(raw, events, disconnect, resync)
      })
    )
      return
  }

  requestResync = resync
  switch (type) {
    case 'TerrainTileVersion': {
      const key = `${data.tile_x},${data.tile_z}`
      terrainSnapshots.delete(key)
      worldView.pendingTerrain.add(key)
      terrainDownloads.set(data)
      break
    }
    case 'WorldUpdate': {
      const update = data as WorldUpdate
      const previousEpoch = worldView.epoch
      if (!worldView.accept(update)) {
        if (!worldView.synchronized) resyncWorld()
        return
      }
      if (update.reset) {
        if (previousEpoch !== worldView.epoch) {
          worldView.staticReady = false
          objectManager.resetWorld()
        }
        housingManager.resetView()
        resetTerrainDownloads()
        resetFences()
        resetEstateStorage()
        dungeonManager.resetDynamicView()
        gameStore.update((state) => {
          for (const id of state.otherPlayers.keys()) {
            stopMusicPerformance(id)
            stopPlayerInstrument(id)
            removeBobber(id)
          }
          state.otherPlayers.clear()
          return state
        })
        remoteEstateInteractions.clear()
        remotePlayerManager.reset()
        monsterManager.reset()
        groundItemManager.reset()
        campfireManager.reset()
        stallManager.reset()
        tipHatManager.reset()
        mealManager.reset()
      }
      for (const event of update.events) {
        if (
          (event.change === 'Leave' || event.change === 'Delete') &&
          event.subject.startsWith('terrain:')
        ) {
          const key = event.subject.slice(8)
          terrainDownloads.remove(key)
          terrainSnapshots.delete(key)
          worldView.pendingTerrain.delete(key)
        }
        for (const message of event.messages) {
          if (
            event.change === 'Leave' &&
            typeof message === 'object' &&
            message &&
            ('PlayerDisappeared' in message || 'MonsterRemoved' in message)
          ) {
            const generation = worldView.generation
            const epoch = worldView.epoch
            const deadline = performance.now() + 10000
            const finish = () => {
              if (
                worldView.epoch !== epoch ||
                worldView.generation !== generation ||
                worldView.subjects.has(event.subject)
              )
                return
              const id = event.subject.slice(event.subject.indexOf(':') + 1)
              const monster = monsterManager.monsters.get(id)
              const target = monster?.targetPosition
              const interpolating = event.subject.startsWith('player:')
                ? remotePlayerManager.isInterpolating(Number(id))
                : !!monster &&
                  !!target &&
                  (monster.state === 'walk' || monster.state === 'run') &&
                  Math.hypot(
                    shortestWrappedDeltaX(monster.position.x, target.x),
                    monster.position.z - target.z
                  ) > 0.2
              if (interpolating && performance.now() < deadline)
                setTimeout(finish, 50)
              else handleServerMessage(message, events, disconnect, resync)
            }
            setTimeout(finish, 0)
          } else handleServerMessage(message, events, disconnect, resync)
        }
      }
      if (update.reset && worldView.synchronized)
        housingManager.completeSnapshot()
      if (!worldView.synchronized) resyncWorld()
      return
    }
    case 'AuthSuccess': {
      const characters = (data.characters as AccountCharacter[]) ?? []
      setCapeUploadToken(data.cape_upload_token || null)
      events.authSuccess.emit({
        accountName: data.account_name,
        characters,
      })
      break
    }

    case 'AuthError': {
      console.warn('Authentication error:', data.message)
      events.authError.emit(data.message)
      break
    }

    case 'JoinSuccess': {
      resetTeleportEffects()
      remoteEstateInteractions.clear()
      worldView.synchronized = false
      housingManager.resetView()
      scheduleResync()
      manaState.set(null)
      resetFences()
      resetHousePlacement()
      resetEstateStorage()
      const serverPlayer: ServerPlayer = data.player
      console.log('Join successful, received player data:', serverPlayer)
      isAdminUser.set(data.is_admin === true)
      const player = toLocalPlayer(serverPlayer)
      gameStore.update((state) => ({
        ...state,
        currentPlayer: player,
      }))
      // Players who logged out inside a dungeon reconnect there.
      syncOwnFloor(
        serverPlayer.floor_level,
        serverPlayer.position.x,
        serverPlayer.position.z
      )
      events.joinSuccess.emit()
      break
    }

    case 'CharacterCreated': {
      const character: AccountCharacter = data.character
      events.characterCreated.emit(character)
      break
    }

    case 'CharacterStatsRolled': {
      const attributes: CharacterAttributes = data.attributes
      events.characterStatsRolled.emit({
        attributes,
        maxHp: data.max_hp,
      })
      break
    }

    case 'CharacterDeleted': {
      events.characterDeleted.emit(data.character_id)
      break
    }

    case 'CharacterRenameRequired': {
      events.characterRenameRequired.emit(data.character_id)
      break
    }

    case 'CharacterRenamed': {
      events.characterRenamed.emit({
        characterId: data.character_id,
        name: data.name,
      })
      break
    }

    case 'CharacterError': {
      events.characterError.emit(data.message)
      break
    }

    case 'PlayerJoined': {
      const serverPlayer: ServerPlayer = data.player
      const player = toLocalPlayer(serverPlayer)
      let joinedName: string | null = null
      gameStore.update((state) => {
        if (!state.currentPlayer) {
          console.log('Setting current player from PlayerJoined:', player)
          return { ...state, currentPlayer: player }
        } else if (serverPlayer.id !== state.currentPlayer.id) {
          addRemotePlayerToState(state, serverPlayer)
          joinedName = serverPlayer.name
        }
        return state
      })
      if (joinedName) {
        addChatMessage({
          text: translate('system.playerJoined', { name: joinedName }),
          sender: 'system',
        })
      }
      break
    }

    case 'PlayerAppeared': {
      finishTeleportArrival(data.player.id)
      const serverPlayer: ServerPlayer = data.player
      gameStore.update((state) => {
        if (serverPlayer.id === state.currentPlayer?.id) {
          state.currentPlayer = {
            ...state.currentPlayer,
            ...toLocalPlayer(serverPlayer),
          }
          syncOwnFloor(
            serverPlayer.floor_level,
            serverPlayer.position.x,
            serverPlayer.position.z
          )
        } else {
          addRemotePlayerToState(state, serverPlayer)
        }
        return state
      })
      break
    }

    case 'PlayerLeft': {
      stopMusicPerformance(data.player_id)
      stopPlayerInstrument(data.player_id)
      let leftName: string | null = null
      gameStore.update((state) => {
        const player = state.otherPlayers.get(data.player_id)
        removeRemotePlayerFromState(state, data.player_id)
        if (player) {
          leftName = player.name
        }
        return state
      })
      if (leftName) {
        addChatMessage({
          text: translate('system.playerLeft', { name: leftName }),
          sender: 'system',
        })
      }
      break
    }

    case 'PlayerDisappeared': {
      // Out of earshot by distance: their tune fades rather than cuts.
      fadeOutMusicPerformance(data.player_id)
      stopPlayerInstrument(data.player_id)
      gameStore.update((state) => {
        removeRemotePlayerFromState(state, data.player_id)
        return state
      })
      break
    }

    case 'PlayerMoved': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      const deckY = bridgeManager.findDeckYAt(
        data.position.x,
        data.position.z,
        null
      )
      remotePlayerManager.setTargetPosition(
        data.player_id,
        {
          x: data.position.x,
          y: deckY ?? data.position.y,
          z: data.position.z,
        },
        data.rotation,
        data.sprinting === true
      )
      const existing = state.otherPlayers.get(data.player_id)
      if (existing && existing.floorLevel !== data.floor_level) {
        updatePlayer(data.player_id, { floorLevel: data.floor_level })
      }
      break
    }

    case 'PlayerMovePath': {
      events.movePath.emit(data)
      break
    }
    case 'PlayerMoveProgress': {
      events.moveProgress.emit(data)
      break
    }
    case 'MountRecovery': {
      events.mountRecovery.emit(data)
      break
    }

    case 'MovementResync':
    case 'PositionCorrected': {
      if (type === 'PositionCorrected') {
        if (performance.now() - lastCorrection < 3000) resyncWorld()
        lastCorrection = performance.now()
      }
      syncOwnFloor(data.floor_level, data.position.x, data.position.z)
      events.positionCorrected.emit({
        x: data.position.x,
        y: data.position.y,
        z: data.position.z,
        rotation: data.rotation,
        resyncId: type === 'MovementResync' ? data.resync_id : undefined,
      })
      break
    }

    case 'PlayerTeleportEffect': {
      const local = get(gameStore).currentPlayer?.id === data.player_id
      playTeleportEffect(
        {
          playerId: data.player_id,
          position: data.position,
          floorLevel: data.floor_level,
          phase: data.phase,
        },
        local
      )
      if (data.phase === 'Departing') {
        const remote = remotePlayerManager.players.get(data.player_id)
        if (remote)
          remotePlayerManager.teleportPlayer(
            data.player_id,
            data.position,
            remote.rotation
          )
      }
      break
    }

    case 'PlayerTeleported': {
      finishTeleportArrival(data.player_id)
      const state = get(gameStore)
      if (state.currentPlayer && state.currentPlayer.id === data.player_id) {
        cancelLocalTeleportRequest()
        // Through the store, not a bare mutation: subscribers that live
        // across a teleport (HUD widgets) otherwise keep the old position.
        gameStore.update((s) => {
          s.currentPlayer?.position.set(
            data.position.x,
            data.position.y,
            data.position.z
          )
          return s
        })
        syncOwnFloor(data.floor_level, data.position.x, data.position.z)
        requestCameraReset()
        // Any teleport settles the summon toast — an accepted one succeeded,
        // and one surviving the player's own departure would mislead.
        pendingPartySummons.set([])
        break
      }
      const tpDeckY = bridgeManager.findDeckYAt(
        data.position.x,
        data.position.z,
        null
      )
      remotePlayerManager.teleportPlayer(
        data.player_id,
        tpDeckY !== null ? { ...data.position, y: tpDeckY } : data.position,
        data.rotation
      )
      gameStore.update((state) => {
        const player = state.otherPlayers.get(data.player_id)
        if (player) player.floorLevel = data.floor_level
        return state
      })
      break
    }

    case 'ChatMessage': {
      logSpokenLine(data.player_id, data.message)
      addChatBubble(data.player_id, data.message)
      break
    }

    case 'Recital': {
      if (data.logged) logSpokenLine(data.player_id, data.line)
      // Held until the next verse replaces it.
      addChatBubble(data.player_id, data.line, RECITAL_BUBBLE_MS)
      break
    }

    case 'WhisperMessage': {
      // No chat bubble — a whisper is private.
      const own = get(gameStore).currentPlayer?.name
      addChatMessage(whisperChatEntry(data.from, data.to, data.message, own))
      break
    }

    case 'PartyChatMessage':
      // No chat bubble — the party channel is private to the party.
      addChatMessage(partyChatEntry(data.from, data.message))
      break

    case 'SystemMessage':
      addChatMessage({
        text: data.message,
        localization: data.localization,
        autoTranslate: true,
        sender: 'system',
      })
      break

    case 'PartyInviteReceived':
      enqueueConsent(
        pendingPartyInvites,
        MAX_PENDING_PARTY_INVITES,
        (invite) => invite.inviterId === data.inviter_id,
        {
          inviterId: data.inviter_id,
          inviterName: data.inviter_name,
          offeredAt: Date.now(),
        }
      )
      break

    case 'PartyInviteResult':
      addChatMessage({
        text: data.message,
        autoTranslate: true,
        sender: 'system',
      })
      break

    case 'PlayerTradeRequested':
      enqueueTradeRequest(data.requester_id, data.requester_name)
      break

    case 'PlayerTradeRequestResult':
      addChatMessage({
        text: data.message,
        autoTranslate: true,
        sender: 'system',
      })
      break

    case 'PlayerTradeUpdate':
      dismissTradeRequest(data.state.them.player_id)
      playerTrade.set(data.state)
      break

    case 'PlayerTradeEnded':
      playerTrade.set(null)
      playerTradeError.set(null)
      addChatMessage({
        text: data.message,
        autoTranslate: true,
        sender: 'system',
      })
      break

    case 'PlayerTradeError':
      playerTradeError.set(data.message)
      break

    case 'PartySummonReceived': {
      // Replace any same-caster entry (always stale: the ack-only cast never
      // re-sends for a live one) and age out the dead. No cap — distinct
      // casters bound the queue at the party size.
      const now = Date.now()
      pendingPartySummons.update((queue) => [
        ...queue.filter(
          (s) =>
            now - s.offeredAt < SUMMON_TTL_MS && s.casterId !== data.caster_id
        ),
        {
          casterId: data.caster_id,
          casterName: data.caster_name,
          offeredAt: now,
        },
      ])
      break
    }

    case 'PartyState': {
      const members = data.members as PartyMemberEntry[]
      const joined = members.length > 0
      partyRoster.set(joined ? { leaderId: data.leader_id, members } : null)
      if (joined) {
        pendingPartyInvites.set([])
      } else {
        resetPartyPositions()
      }
      // A summons only lives while its caster shares the roster — one from
      // someone who left can only ever be answered with "faded".
      const rosterIds = new Set(members.map((m) => m.id))
      pendingPartySummons.update((queue) =>
        queue.filter((summon) => rosterIds.has(summon.casterId))
      )
      break
    }

    case 'PartyVitals':
      applyPartyVitals(data.members as PartyMemberVitalsEntry[])
      break

    case 'FriendList':
      applyFriendList(
        (
          data.friends as {
            character_id: number
            name: string
            level: number
            class: CharacterClass
          }[]
        ).map((f) => ({
          characterId: f.character_id,
          name: f.name,
          level: f.level,
          class: f.class,
        }))
      )
      break

    case 'FriendsOnline': {
      const announced = applyFriendsOnline(
        data.friends as { character_id: number; level: number }[],
        get(friendList)
      )
      if (get(friendOnlineNoticeEnabled)) {
        for (const name of announced) {
          addChatMessage({
            text: translate('system.friendOnline', { name }),
            sender: 'system',
          })
        }
      }
      break
    }

    case 'FriendRequestReceived':
      enqueueConsent(
        pendingFriendRequests,
        MAX_PENDING_FRIEND_REQUESTS,
        (request) => request.requesterId === data.requester_id,
        {
          requesterId: data.requester_id,
          requesterName: data.requester_name,
          offeredAt: Date.now(),
        }
      )
      break

    case 'PartyPositions':
      applyPartyPositions(
        data.members as PartyMemberPositionEntry[],
        get(gameStore).currentPlayer?.id,
        get(partyRoster) !== null
      )
      break

    case 'GameState':
      // A join snapshot starts a fresh session: any party membership died
      // with the old one (in-memory, disconnect = leave), and the server
      // cannot re-send what no longer exists.
      resetPartyStores()
      // Friendships persist, but this session's roster arrives as its own
      // FriendList; anything held from the old one is stale.
      resetFriendStores()
      gameStore.update((state) => {
        state.otherPlayers.clear()
        remoteEstateInteractions.clear()
        remotePlayerManager.reset()
        // A list, not a map: player ids are numeric and the wasm serializer
        // rejects non-string map keys (see ServerMessage::GameState).
        const serverPlayers = data.players as ServerPlayer[]
        serverPlayers.forEach((serverPlayer) => {
          if (serverPlayer.id !== state.currentPlayer?.id) {
            const player = toRemotePlayer(serverPlayer)
            remotePlayerManager.initPlayer(
              serverPlayer.id,
              serverPlayer.position,
              serverPlayer.rotation
            )
            if (serverPlayer.object_type) {
              applyObjectInteraction(
                serverPlayer.id,
                serverPlayer.object_type,
                serverPlayer.position.x,
                serverPlayer.position.z,
                serverPlayer.object_id
              )
            }
            state.otherPlayers.set(serverPlayer.id, player)
          }
        })
        refreshBardZone(state.otherPlayers)
        return state
      })

      monsterManager.reset()
      if (data.monsters) {
        Object.values(data.monsters as Record<string, ServerMonster>).forEach(
          (monster) => monsterManager.spawnWithId(monster)
        )
      }

      groundItemManager.reset()
      if (data.ground_items) {
        ;(data.ground_items as ServerGroundItem[]).forEach((item) => {
          groundItemManager.spawn(item)
        })
      }

      campfireManager.reset()
      if (data.campfires) {
        for (const campfire of data.campfires) campfireManager.spawn(campfire)
      }
      stallManager.reset()
      if (data.stalls) {
        for (const stall of data.stalls) stallManager.spawn(stall)
      }
      tipHatManager.reset()
      if (data.tip_hats) {
        for (const hat of data.tip_hats) tipHatManager.spawn(hat)
      }
      mealManager.reset()
      if (data.meals) {
        for (const meal of data.meals) mealManager.spawn(meal)
      }
      break

    case 'GameTimeSync': {
      setServerGameTime({
        year: data.datetime.year,
        month: data.datetime.month,
        day: data.datetime.day,
        hour: data.datetime.hour,
        minute: data.datetime.minute,
        isNight: data.is_night,
      })
      break
    }
    case 'WeatherSync': {
      setWeather({
        seed: data.seed,
        bias: data.bias,
        sectorsTag: data.sectors_tag,
        rainOverride: data.rain_override ?? null,
      })
      break
    }

    case 'MonsterSpawned': {
      monsterManager.spawnWithId(data.monster as ServerMonster)
      break
    }

    case 'MonsterMoved':
      monsterManager.updateMonsterFromNetwork(
        data.monster_id,
        data.position,
        data.rotation,
        data.state,
        data.target_position,
        data.chasing
      )
      break

    case 'MonsterRemoved':
      monsterManager.remove(data.monster_id)
      break

    case 'MonsterDead':
      monsterManager.handleMonsterDead(
        data.monster_id,
        data.dropped_weapon_item_def_id
      )
      break

    case 'PlayerAttacked': {
      if (data.dagger_strike == null) {
        clearDaggerCast(data.player_id)
        remotePlayerManager.handleAttack(data.player_id)
      }

      const gameState = get(gameStore)
      const isLocalAttacker = gameState.currentPlayer?.id === data.player_id
      const attackerName = isLocalAttacker
        ? translate('chat.you')
        : gameState.otherPlayers.get(data.player_id)?.name ||
          translate('chat.unknown')

      addCombatMessage({
        text: attackLog(data.roll, data.hit, data.damage, data.dagger_strike),
        sender: isLocalAttacker ? 'local' : 'remote',
        name: attackerName,
        hit: data.hit,
      })

      monsterManager.handleMonsterAttacked(
        data.monster_id,
        data.player_id,
        data.hit,
        data.damage,
        data.ammo_item_def_id,
        data.dagger_strike != null
      )
      break
    }

    case 'DaggerDoubleSlashStarted': {
      const local = get(gameStore).currentPlayer?.id === data.player_id
      if (local) {
        acknowledgeDaggerSkill(data.cooldown_ms)
      } else {
        playDaggerSkill(data.player_id)
        remotePlayerManager.handleAttack(data.player_id)
      }
      break
    }

    case 'DaggerDoubleSlashSkipped': {
      const state = get(gameStore)
      const local = state.currentPlayer?.id === data.player_id
      addCombatMessage({
        text: daggerSkippedLog(data.strike, data.reason),
        sender: local ? 'local' : 'remote',
        name: local
          ? translate('chat.you')
          : state.otherPlayers.get(data.player_id)?.name ||
            translate('chat.unknown'),
        hit: false,
      })
      break
    }

    case 'DaggerDoubleSlashRejected': {
      const playerId = get(gameStore).currentPlayer?.id
      if (playerId !== undefined) clearDaggerCast(playerId)
      if (data.cooldown_ms > 0) acknowledgeDaggerSkill(data.cooldown_ms)
      else
        daggerSkillState.update((state) => ({
          ...state,
          pending: false,
          queued: false,
        }))
      const reasonKey = DAGGER_REJECTION_MESSAGES[data.reason]
      reportSkillFailure(
        data.reason === 'dagger_required' || data.reason === 'rogue_required'
          ? abilityRequirementsNotMet(abilityDisplayName(DOUBLE_SLASH.id))
          : translate('combat.doubleSlashRejected', {
              reason: reasonKey ? translate(reasonKey) : data.reason,
            })
      )
      break
    }

    case 'PlayerAttackRejected': {
      if (data.reason === 'invalid_target') {
        if (combatController.targetMonsterId === data.monster_id) {
          combatController.cancelCombat()
        }
        const monster = monsterManager.monsters.get(data.monster_id)
        // Corpse rejections must not cut short impact or death animations.
        if (monster && monster.state !== 'dead' && !monster.isDeadPending) {
          monsterManager.remove(data.monster_id)
        }
      }
      if (data.reason === 'out_of_ammo') {
        combatController.cancelCombat()
      }
      const reasonKey = ATTACK_REJECTION_MESSAGES[data.reason]
      addCombatMessage({
        text: translate('combat.attackRejected', {
          reason: reasonKey ? translate(reasonKey) : data.reason,
        }),
        sender: 'local',
        name: translate('chat.you'),
        hit: false,
      })
      break
    }

    case 'MonsterAttackedPlayer': {
      const gameState = get(gameStore)
      const isCurrentPlayer = gameState.currentPlayer?.id === data.player_id
      const target = isCurrentPlayer
        ? gameState.currentPlayer
        : gameState.otherPlayers.get(data.player_id)
      const targetPos = isCurrentPlayer
        ? gameState.currentPlayer?.position
        : remotePlayerManager.players.get(data.player_id)?.position
      const monster = monsterManager.monsters.get(data.monster_id)
      monsterManager.handleMonsterAttackStarted(data.monster_id, 250, targetPos)

      const impactDelayMs = monsterManager.getMonsterAttackDamageTextDelayMs(
        data.monster_id
      )

      if (isCurrentPlayer) {
        emitCurrentPlayerDamageInfo(
          data.player_id,
          data.damage,
          data.hit,
          data.current_health,
          impactDelayMs
        )
      }

      if (!data.hit) {
        // Whoosh with the monster's weapon material; unarmed types fall
        // through to the default miss sound.
        const monsterWeapon = monster && getMonsterDef(monster.type)?.weapon
        playSwordMissSound(
          getMaterialMissSoundUrl(
            monsterWeapon ? getItemDef(monsterWeapon)?.material : undefined
          ),
          impactDelayMs
        )
      }
      if (data.hit && data.damage > 0 && target) {
        if (data.current_health <= 0) {
          if (claimPlayerDeath(data.player_id)) {
            playPlayerDeathSound(target.gender)
          }
        } else {
          playPlayerHurtSound(target.gender, impactDelayMs)
          emitPlayerHit(data.player_id, isCurrentPlayer, impactDelayMs)
        }
      }

      updatePlayer(data.player_id, {
        health: data.current_health,
      })

      const monsterTargetName = isCurrentPlayer
        ? translate('chat.you')
        : (target?.name ?? translate('chat.unknown'))
      addCombatMessage({
        text: data.hit
          ? translate('combat.monsterHit', {
              roll: data.roll,
              name: monsterTargetName,
              damage: data.damage,
            })
          : translate('combat.miss', { roll: data.roll }),
        sender: 'system',
        name: translate('combat.monster'),
        hit: data.hit,
      })
      break
    }

    case 'PlayerDead': {
      remoteEstateInteractions.delete(data.player_id)
      console.log('Player dead:', data.player_id)
      stopPlayerInstrument(data.player_id)
      const gameState = get(gameStore)
      const isDeadCurrentPlayer = gameState.currentPlayer?.id === data.player_id
      const deadPlayer = isDeadCurrentPlayer
        ? gameState.currentPlayer
        : gameState.otherPlayers.get(data.player_id)
      const deadPlayerName = isDeadCurrentPlayer
        ? translate('chat.you')
        : (deadPlayer?.name ?? translate('chat.unknown'))
      if (deadPlayer && claimPlayerDeath(data.player_id)) {
        playPlayerDeathSound(deadPlayer.gender)
      }
      addCombatMessage({
        text: isDeadCurrentPlayer
          ? translate('combat.youSlain')
          : translate('combat.playerSlain', { name: deadPlayerName }),
        sender: 'system',
      })

      if (!isDeadCurrentPlayer) {
        remotePlayerManager.handleDead(data.player_id)
      }
      break
    }

    case 'Kicked': {
      console.warn('Kicked from server:', data.reason)
      events.kicked.emit(data.reason)
      resetGameStore()
      monsterManager.reset()
      remoteEstateInteractions.clear()
      remotePlayerManager.reset()
      disconnect()
      break
    }

    case 'ServerNotice': {
      serverNotice.set(data.message ?? null)
      break
    }

    case 'PlayerRespawned': {
      const serverPlayer: ServerPlayer = data.player
      remoteEstateInteractions.delete(serverPlayer.id)
      stopPlayerInstrument(serverPlayer.id)
      console.log('Player respawned:', serverPlayer.id)
      const gameState = get(gameStore)
      const isCurrentPlayerRespawned =
        gameState.currentPlayer?.id === serverPlayer.id

      if (isCurrentPlayerRespawned) {
        const respawnPosition = new Vector3(
          serverPlayer.position.x,
          serverPlayer.position.y,
          serverPlayer.position.z
        )
        updatePlayer(serverPlayer.id, {
          position: respawnPosition,
          health: serverPlayer.health,
          maxHealth: serverPlayer.max_health,
        })
        // A death lands on the inn's floor; a talisman revive stays put.
        syncOwnFloor(
          serverPlayer.floor_level,
          serverPlayer.position.x,
          serverPlayer.position.z
        )
        requestCameraReset()
        addChatMessage({ text: translate('system.revived'), sender: 'system' })
      } else {
        // Respawns now travel across floors (for NPCs tending the sick
        // room); a player this client doesn't render is not ours to move.
        if (!gameState.otherPlayers.has(serverPlayer.id)) break
        updatePlayer(serverPlayer.id, {
          health: serverPlayer.health,
          maxHealth: serverPlayer.max_health,
        })
        addChatMessage({
          text: translate('system.playerRevived', { name: serverPlayer.name }),
          sender: 'system',
        })
        remotePlayerManager.handleRespawn(
          serverPlayer.id,
          serverPlayer.position,
          serverPlayer.rotation
        )
        if (serverPlayer.object_type) {
          applyObjectInteraction(
            serverPlayer.id,
            serverPlayer.object_type,
            serverPlayer.position.x,
            serverPlayer.position.z,
            serverPlayer.object_id
          )
        }
      }
      events.playerRespawned.emit(serverPlayer.id)
      // After the idle transition the emit triggers, so it keeps the pose.
      if (isCurrentPlayerRespawned && serverPlayer.object_type) {
        respawnPoseRequest.set(serverPlayer.object_type)
      }
      break
    }

    case 'PlayerHealthUpdate': {
      const gameState = get(gameStore)
      const isCurrentPlayer = gameState.currentPlayer?.id === data.player_id

      let regenInfo = undefined
      if (isCurrentPlayer && gameState.currentPlayer) {
        const diff = data.health - gameState.currentPlayer.health
        if (diff > 0) {
          const prevTrigger =
            gameState.currentPlayer.lastRegenInfo?.trigger ?? 0
          regenInfo = {
            damage: diff,
            hit: true,
            trigger: prevTrigger + 1,
          }
        }
      }

      updatePlayer(data.player_id, {
        health: data.health,
        maxHealth: data.max_health,
        ...(isCurrentPlayer ? { lastRegenInfo: regenInfo } : {}),
      })
      break
    }

    case 'PlayerTorchToggled': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      updatePlayer(data.player_id, { torchOn: data.enabled })
      break
    }
    case 'PlayerRadianceToggled':
      updatePlayer(data.player_id, { radianceOn: data.enabled })
      break

    case 'PlayerMountChanged': {
      updatePlayer(data.player_id, { mount: data.mount })
      break
    }

    case 'PlayerWetToggled': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      updatePlayer(data.player_id, { wet: data.wet })
      break
    }

    case 'PlayerTitleChanged': {
      updatePlayer(data.player_id, { title: data.title ?? null })
      break
    }

    case 'PlayerTitles': {
      earnedTitles.set(data.titles ?? [])
      const state = get(gameStore)
      if (state.currentPlayer) {
        updatePlayer(state.currentPlayer.id, { title: data.active ?? null })
      }
      break
    }

    case 'TitleEarned': {
      addChatMessage({
        text: translate('system.titleEarned', {
          title: titleNameNow(data.title),
        }),
        sender: 'system',
      })
      break
    }

    case 'PlayerMainHandChanged': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      updatePlayer(data.player_id, { mainHand: data.item_def_id ?? null })
      break
    }

    case 'CapeDyePrompt': {
      capeDyeDialog.set({ instanceId: data.instance_id })
      break
    }

    case 'LandClaimPrompt': {
      resetHousePlacement()
      applyLandClaimPreview(data)
      break
    }
    case 'LandscapingMode':
      stopEstateChestMode()
      resetHousePlacement()
      openLandscapingMode(data)
      inventoryVisible.set(false)
      fenceError.set(null)
      break
    case 'LandscapingPaletteUnlocked':
      landscapingMode.update((mode) =>
        mode ? { ...mode, palette: data.palette } : null
      )
      break
    case 'LandscapeChanged':
      for (const tile of data.tiles as LandscapingTile[]) {
        get(editorSplatManager)?.setSplatmap(
          tile.tile_x,
          tile.tile_z,
          new Uint8Array(tile.splat)
        )
        const mask = new Uint8Array(tile.cleared)
        get(editorGrassDataManager)?.applyLandscapingMask(
          tile.tile_x,
          tile.tile_z,
          mask
        )
        get(editorTreeDataManager)?.applyLandscapingMask(
          tile.tile_x,
          tile.tile_z,
          mask
        )
      }
      break
    case 'LandscapeInvalidated':
      for (const [tx, tz] of data.tiles as [number, number][]) {
        get(editorSplatManager)?.invalidateLandscaping(tx, tz)
        get(editorGrassDataManager)?.invalidateLandscaping(tx, tz)
        get(editorTreeDataManager)?.invalidateLandscaping(tx, tz)
      }
      break
    case 'LandscapeEditResult':
      landscapingPending.set(false)
      landscapingError.set(data.error ?? null)
      break
    case 'FenceVisibility':
      applyFenceVisibility(data.added, data.removed)
      break
    case 'FenceEditResult':
      fencePending.set(false)
      fenceError.set(data.error ?? null)
      break
    case 'EstateChestMode':
      stopFenceMode()
      selectedEstateFurniture.set(null)
      startEstateFurniturePlacement({ ...data, kind: 'place' })
      inventoryVisible.set(false)
      break
    case 'EstateFurnitureMoveMode':
      if (get(selectedEstateFurniture)?.id !== data.furniture.id) break
      stopFenceMode()
      startEstateFurniturePlacement({
        kind: 'move',
        furniture: data.furniture,
        item_def_id: data.furniture.item_def_id,
        owner_id: data.furniture.owner_id,
        plots: data.plots,
      })
      inventoryVisible.set(false)
      break
    case 'EstateChestVisibility':
      applyEstateChestVisibility(data.added, data.removed)
      for (const [playerId, interaction] of remoteEstateInteractions) {
        if (
          !(data.added as EstateChest[]).some(
            (chest) =>
              chest.id === interaction.objectId &&
              chest.item_def_id === interaction.objectType
          )
        )
          continue
        const player = remotePlayerManager.players.get(playerId)
        if (player)
          void applyObjectInteraction(
            playerId,
            interaction.objectType,
            player.position.x,
            player.position.z,
            interaction.objectId
          )
      }
      break
    case 'EstateChestEditResult':
      applyEstateFurnitureEditResult(data.error ?? null)
      break
    case 'FurniturePurchaseResult':
      furniturePurchasePending.set(false)
      furnitureShopError.set(data.error ?? null)
      if (!data.error) clearFurnitureBasket()
      break
    case 'EstateChestState':
      estateChestPending.set(false)
      if (data.error) estateChestError.set(data.error)
      if (data.state) {
        inventoryVisible.set(false)
        openEstateChest.set(data.state)
      }
      break

    case 'LandClaimed': {
      landClaimDialog.update((claim) =>
        claim ? { ...claim, status: 'claimed' } : null
      )
      addChatMessage({
        text: translate('system.landClaimed'),
        sender: 'system',
      })
      break
    }

    case 'LandRejected': {
      landClaimDialog.update((claim) =>
        claim ? { ...claim, status: 'rejected', reason: data.reason } : null
      )
      addChatMessage({
        text: data.reason,
        autoTranslate: true,
        sender: 'system',
      })
      break
    }

    case 'CapeTexturePrompt': {
      capeTextureDialog.set({ instanceId: data.instance_id })
      break
    }

    case 'PlayerBackChanged': {
      if (isSelfPlayer(data.player_id)) break
      updatePlayer(data.player_id, {
        back: data.item_def_id ?? null,
        backColor: data.cape_color ?? null,
        backTexture: data.cape_texture ?? null,
      })
      break
    }

    case 'PlayerMusicStarted': {
      const isMe = isSelfPlayer(data.player_id)
      stopPlayerInstrument(data.player_id)
      startMusicPerformance(data.player_id, data.track, isMe, data.elapsed_secs)
      // Our own /play_music went to the server unresolved; its reply names
      // the track and is what strikes up our emote.
      if (isMe) emoteRequest.set(MUSIC_EMOTE_ANIM)
      const who = isMe
        ? null
        : (get(gameStore).otherPlayers.get(data.player_id)?.name ??
          translate('chat.someone'))
      addChatMessage({
        text: who
          ? translate('system.playerPlayMusic', {
              name: who,
              track: data.track,
            })
          : translate('system.playMusic', { track: data.track }),
        sender: 'system',
      })
      break
    }

    case 'PlayerInstrumentStarted': {
      clearInstrumentNoteTimers(data.player_id)
      stopInstrumentPerformer(data.player_id)
      if (isSelfPlayer(data.player_id)) {
        emotePanelVisible.set(false)
        openInstrumentPanel()
        emoteRequest.set(MUSIC_EMOTE_ANIM)
      } else {
        holdLiveInstrumentQuiet()
      }
      // Quiet the playlist first, or stopMusicPerformance restarts it under
      // the live session.
      stopMusicPerformance(data.player_id)
      break
    }

    case 'PlayerInstrumentNotes': {
      playRemoteInstrumentNotes(
        data.player_id,
        data.position,
        data.floor_level,
        data.events
      )
      break
    }

    case 'PlayerInteractionChanged': {
      // Leaving the strum ends the tune, for the performer too.
      applyInteractionChange(data.player_id, data.object_type ?? null)
      if (data.object_type !== MUSIC_EMOTE_ANIM) {
        stopPlayerInstrument(data.player_id)
      }
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        if (!data.object_type) emoteStopRequest.set(true)
        // Our own /emote went to the server unresolved; this broadcast is
        // its reply, the way PlayerMusicStarted starts /play_music.
        if (data.object_type && SLASH_EMOTE_ANIMS.has(data.object_type)) {
          emoteRequest.set(data.object_type)
        }
        break
      }
      const ft: string | null = data.object_type ?? null
      if (ft) {
        const rp = remotePlayerManager.players.get(data.player_id)
        const wx = rp?.position.x ?? 0
        const wz = rp?.position.z ?? 0
        applyObjectInteraction(data.player_id, ft, wx, wz, data.object_id)
      } else {
        remoteEstateInteractions.delete(data.player_id)
        remotePlayerManager.handleStopInteraction(data.player_id)
      }
      break
    }

    case 'InteractionRejected': {
      // The event only cancels an in-flight interaction animation, so the
      // refusal would otherwise be silent. Reasons are sentences except the
      // machine codes mapped here (same pattern as PlayerAttackRejected).
      const reasonText: Record<string, string> = {
        occupied: translate('system.occupied'),
      }
      addChatMessage({
        text: reasonText[data.reason] ?? data.reason,
        autoTranslate: !reasonText[data.reason],
        sender: 'system',
      })
      events.interactionRejected.emit(data.reason)
      break
    }

    case 'DungeonChestOpened': {
      // No items + no gold = re-open of a chest already claimed tonight;
      // the lid still swings, showing an empty box.
      if (dungeonManager.markTreasureChestOpened(data.entrance_id))
        playPropSound('chestOpen')
      const empty = (data.item_def_ids as string[]).length === 0 && !data.gold
      addChatMessage({
        text: empty
          ? translate('system.chestEmpty')
          : translate('system.chestOpened', {
              name: actorName(data.player_id),
              gold: data.gold,
            }),
        sender: 'system',
      })
      break
    }

    case 'DungeonPropsState':
      dungeonManager.setPropsState(
        data.entrance_id,
        data.depth,
        data.broken,
        data.opened
      )
      break

    // Snapshots reconcile silently; only live broadcasts play prop sounds.
    case 'DungeonPropBroken': {
      const isNew = dungeonManager.markPropBroken(
        data.entrance_id,
        data.depth,
        data.prop_id
      )
      const selfBreak = dungeonManager.consumeSelfBreak(
        data.depth,
        data.prop_id
      )
      if (isNew && !selfBreak) playPropSound('break')
      break
    }

    case 'DungeonPropOpened': {
      const isNew = dungeonManager.markPropOpened(
        data.entrance_id,
        data.depth,
        data.prop_id
      )
      if (isNew) playPropSound('chestOpen')
      break
    }

    case 'DungeonDoorToggled':
      dungeonManager.applyDoorToggle(
        data.entrance_id,
        data.depth,
        data.door_id,
        data.is_open
      )
      break

    case 'DungeonDoorState':
      dungeonManager.applySubjectDoor(
        data.entrance_id,
        data.depth,
        data.door_id,
        data.is_open
      )
      break

    case 'DungeonPropState':
      dungeonManager.applySubjectProp(
        data.entrance_id,
        data.depth,
        data.prop_id,
        data.active,
        data.broken,
        data.opened
      )
      break

    case 'DungeonDoorsState':
      dungeonManager.applyDoorsSnapshot(data.entrance_id, data.doors)
      break

    case 'DungeonDiscoveries':
      discoveredDungeonIds.set(new Set(data.entrance_ids as string[]))
      break

    case 'HouseSpawned':
      housingManager.handleRemoteHouseSpawned(data.house)
      break

    case 'HousePlacementStarted':
      landClaimDialog.set(null)
      selectLandscapingTool('House')
      openHousePlacement(
        data.instance_id,
        data.item_name,
        data.house,
        data.plots
      )
      break

    case 'HousePlacementResult':
      applyHousePlacementResult(data.error ?? null)
      break

    case 'HouseDemolitionResult':
      applyHouseDemolitionResult(data.house_id, data.error ?? null)
      break

    case 'HouseUpdated':
      housingManager.handleRemoteHouseSpawned(data.house)
      break

    case 'HeightTilesInvalidated': {
      const heightManager = get(editorHeightManager)
      if (heightManager) {
        pendingHeightTileRefresh = pendingHeightTileRefresh
          .catch(() => {})
          .then(() => heightManager.refreshTiles(data.tiles ?? []))
        void pendingHeightTileRefresh.catch((error) =>
          console.warn('Failed to refresh terrain height tiles:', error)
        )
      }
      break
    }

    case 'TreeTilesInvalidated': {
      const treeDataManager = get(editorTreeDataManager)
      if (treeDataManager) {
        void pendingHeightTileRefresh
          .catch(() => {})
          .then(() => treeDataManager.refreshTiles(data.tiles ?? []))
      }
      break
    }

    case 'GrassTilesInvalidated': {
      const grassDataManager = get(editorGrassDataManager)
      if (grassDataManager) {
        void pendingHeightTileRefresh
          .catch(() => {})
          .then(() => grassDataManager.refreshTiles(data.tiles ?? []))
      }
      break
    }

    case 'HouseRemoved':
      housingManager.handleRemoteHouseRemoved(data.house_id)
      break

    case 'HousesInArea':
      housingManager.handleRemoteHousesBatch(data.houses)
      break

    case 'DoorToggled':
      housingManager.handleDoorToggled(
        data.house_id,
        data.room_index,
        data.wall_dir,
        data.segment_index,
        data.is_open
      )
      break

    case 'EquipmentEnchantSucceeded':
      queueEnchantSuccess(data.player_id, data.weapon)
      break

    case 'InventoryState':
      setInventory(data.inventory)
      break
    case 'InventoryUpdated':
      setInventory(data.inventory)
      break

    case 'GroundItemSpawned': {
      const item = data.item as ServerGroundItem
      groundItemManager.spawn(item, { animateSpawn: true })
      // Only what a hand put down: loot announces itself by landing.
      announceGroundItem(
        item.dropped_by,
        item.item_def_id,
        'droppedItem',
        item.quantity
      )
      break
    }

    case 'GroundItemAppeared':
      groundItemManager.spawn(data.item as ServerGroundItem)
      break

    case 'GroundItemRemoved': {
      // Read the pile before the removal drops it — who looted what matters
      // in a party, where one bag takes the drop everybody fought for.
      const taken =
        data.picked_up_by != null
          ? groundItemManager.items.get(data.instance_id)
          : undefined
      groundItemManager.remove(data.instance_id)
      // Self currency pickups: the server's system line reports the payout.
      const selfCurrency =
        taken != null &&
        getItemDef(taken.itemDefId)?.category === 'currency' &&
        isSelfPlayer(data.picked_up_by)
      if (!selfCurrency) {
        announceGroundItem(
          data.picked_up_by,
          taken?.itemDefId,
          'pickedUpItem',
          taken?.quantity
        )
      }
      break
    }

    case 'GroundItemQuantityChanged': {
      const pile = groundItemManager.items.get(data.instance_id)
      groundItemManager.setQuantity(data.instance_id, data.quantity)
      // The picker already got the server's took-X-left-Y system line.
      if (pile && !isSelfPlayer(data.picked_up_by)) {
        announceGroundItem(
          data.picked_up_by,
          pile.itemDefId,
          'pickedUpItem',
          pile.quantity - data.quantity
        )
      }
      break
    }

    case 'ShopState': {
      const session = {
        merchantPlayerId: data.merchant_player_id,
        merchantName: data.merchant_name,
        catalog: data.catalog ?? [],
        sellRatePercent: data.sell_rate_percent,
        priceIndexPercent: data.price_index_percent ?? 100,
        wishlist: data.wishlist ?? [],
        stock: (data.stock ?? []).map(
          (entry: { item_def_id: string; quantity: number }) => ({
            itemDefId: entry.item_def_id,
            quantity: entry.quantity,
          })
        ),
        buyback: mapBuyback(data.buyback),
      }
      setMerchantDeals(data.merchant_player_id, data.active_deals ?? [])
      // Open directly only when the player asked for this shop (or it's a
      // refresh of the one already on screen). An NPC-pushed open_trade is
      // an *offer*: the window covers much of the screen, so it just shows
      // a small accept/decline toast instead of hijacking the view.
      const current = get(shopSession)
      if (
        wasShopRequested(data.merchant_player_id) ||
        current?.merchantPlayerId === data.merchant_player_id
      ) {
        shopSession.set(session)
      } else {
        pendingTradeOffer.set({ session, offeredAt: Date.now() })
      }
      break
    }

    case 'LandAccountState': {
      if (get(shopSession)?.merchantPlayerId !== data.merchant_player_id) break
      landTransferPending.set(false)
      landAccountError.set(data.error ?? null)
      if (!data.error) landAccount.set(data)
      break
    }

    case 'GoldUpdate':
      playerGold.set(Number(data.gold))
      break

    case 'EffectiveStatsUpdated':
      playerEffectiveStats.set({
        guard: Number(data.guard),
        cha: Number(data.cha),
      })
      break

    case 'GoldGained': {
      const state = get(gameStore)
      const playerId = state.currentPlayer?.id
      if (playerId) {
        updatePlayer(playerId, {
          lastGoldInfo: {
            amount: Number(data.amount),
            trigger: (state.currentPlayer?.lastGoldInfo?.trigger ?? 0) + 1,
          },
        })
      }
      break
    }

    case 'TradeError':
      addChatMessage({
        text: data.message,
        localization: data.localization,
        autoTranslate: true,
        sender: 'system',
      })
      break

    case 'DealUpdated':
      applyDealUpdate(
        data.merchant_player_id,
        data.item_def_id,
        data.kind,
        data.modifier_pct,
        data.expires_in_secs
      )
      break

    case 'BuybackUpdated':
      shopSession.update((session) =>
        session && session.merchantPlayerId === data.merchant_player_id
          ? { ...session, buyback: mapBuyback(data.buyback) }
          : session
      )
      break

    case 'XpGained': {
      const gameState = get(gameStore)
      const previousPlayer = gameState.currentPlayer
      const previousLevel =
        previousPlayer && previousPlayer.id === data.player_id
          ? previousPlayer.level
          : null
      const isCurrentPlayer = previousPlayer?.id === data.player_id
      const newTotalXp = Number(data.total_xp)
      const xpLost = Number(data.xp_lost ?? 0)
      // Concurrent kill shares can leave the server out of XP order, so a late
      // notice may carry an older total. Keep the gain message, but never roll
      // the displayed XP or level backwards on it.
      const isStaleGain =
        xpLost === 0 &&
        isCurrentPlayer &&
        newTotalXp < (previousPlayer?.totalXp ?? 0)

      let regenInfo = undefined
      if (isCurrentPlayer && previousPlayer) {
        const diff = data.current_hp - previousPlayer.health
        if (diff > 0) {
          const prevTrigger = previousPlayer.lastRegenInfo?.trigger ?? 0
          regenInfo = {
            damage: diff,
            hit: true,
            trigger: prevTrigger + 1,
          }
        }
      }

      // A kill's XP lands on the badge as that monster starts going down, so
      // the gauge spark rides the death animation instead of the packet.
      const killedId: string | null = data.monster_id ?? null
      const heldForKill =
        isCurrentPlayer &&
        !isStaleGain &&
        data.xp_amount > 0 &&
        killedId !== null &&
        monsterManager.isDeathPending(killedId)
          ? killedId
          : null
      // An immediate change (the death penalty) must land on top of a held
      // kill, never under it.
      if (isCurrentPlayer && !heldForKill) releaseXpArrival()
      updatePlayer(data.player_id, {
        ...(isStaleGain || heldForKill
          ? {}
          : { level: data.new_level, totalXp: newTotalXp }),
        health: data.current_hp,
        maxHealth: data.max_hp,
        ...(isCurrentPlayer ? { lastRegenInfo: regenInfo } : {}),
      })
      // Held XP takes its combat-log lines with it, so the badge, the
      // character panel and the chat all turn over on the same beat.
      const lines: string[] = []
      if (data.xp_amount > 0) {
        lines.push(translate('combat.xpGained', { amount: data.xp_amount }))
      } else if (previousLevel !== null) {
        lines.push(
          xpLost > 0
            ? translate('combat.xpLost', { amount: xpLost })
            : translate('combat.deathPenalty')
        )
      }
      if (!isStaleGain) {
        if (data.leveled_up) {
          lines.push(translate('combat.levelUp', { level: data.new_level }))
        } else if (previousLevel !== null && data.new_level < previousLevel) {
          lines.push(translate('combat.levelDown', { level: data.new_level }))
        }
      }
      if (heldForKill) {
        const playerId = data.player_id
        queueXpArrival(
          { level: data.new_level, totalXp: newTotalXp, lines },
          heldForKill,
          (xp) => {
            updatePlayer(playerId, { level: xp.level, totalXp: xp.totalXp })
            for (const text of xp.lines) {
              addCombatMessage({ text, sender: 'local' })
            }
          }
        )
      } else {
        for (const text of lines) addCombatMessage({ text, sender: 'local' })
      }
      break
    }

    case 'SkillsUpdate':
      skillsStore.set(data.skills)
      break

    case 'FishingCasted': {
      // The float spends the swing + flight in the air; it splashes down
      // (and first renders) on the same schedule as the splash sound.
      upsertBobber(
        data.player_id,
        data.position,
        FISHING_CAST_SWING_DELAY_MS + fishing_cast_ms()
      )
      if (isSelfPlayer(data.player_id)) {
        myFishing.set({ phase: 'casting' })
        // Whoosh on the visible swing; splash one flight time (CAST_MS) later.
        playFishingSound('cast', FISHING_CAST_SWING_DELAY_MS)
        playFishingSound(
          'splash',
          FISHING_CAST_SWING_DELAY_MS + fishing_cast_ms()
        )
        addCombatMessage({ text: translate('fishing.cast'), sender: 'local' })
      } else {
        // Interact state ignores late moves; apply the server-computed facing.
        remotePlayerManager.handleInteraction(
          data.player_id,
          FishingAnimationName.CAST,
          0,
          undefined,
          data.rotation
        )
      }
      break
    }

    case 'FishingBite': {
      markBobberBite(data.player_id)
      if (isSelfPlayer(data.player_id)) {
        myFishing.set({ phase: 'bite' })
        playFishingSound('plop')
        addCombatMessage({
          text: translate('fishing.bite'),
          sender: 'local',
        })
      }
      break
    }

    case 'FishingFight': {
      updateBobberFight(
        data.player_id,
        data.bobber,
        data.fish_state,
        data.stamina_pct,
        data.stance
      )
      if (isSelfPlayer(data.player_id)) {
        if (get(myFishing).phase === 'bite' && data.trophy) {
          addCombatMessage({
            text: translate('fishing.trophyFight', {
              tension: fishing_trophy_min_tension(),
            }),
            sender: 'local',
          })
        }
        applyFightUpdate(
          data.fish_state,
          data.tension_pct,
          data.stamina_pct,
          data.trophy
        )
      }
      break
    }

    case 'FishingEnded': {
      const caught = data.outcome?.Caught
      if (caught && getItemDef(caught.item_def_id)?.category === 'fish') {
        landFishingCatch(data.player_id, caught)
      } else {
        removeBobber(data.player_id)
      }
      const isSelf = isSelfPlayer(data.player_id)
      if (!isSelf) remotePlayerManager.handleStopInteraction(data.player_id)
      // Bystander celebration: everyone in radius hears about a trophy.
      if (!isSelf && data.outcome?.Caught?.trophy) {
        const { item_def_id, size_cm } = data.outcome.Caught
        const who = actorName(data.player_id)
        const fishName = itemDisplayName(item_def_id)
        addCombatMessage({
          text: translate('fishing.playerTrophy', {
            name: who,
            fish: fishName,
            size: size_cm,
          }),
          sender: 'local',
        })
      }
      if (isSelf) {
        myFishing.set({ phase: 'idle' })
        cancelPendingFishingSounds()
        const outcome = data.outcome
        if (outcome === 'Escaped') {
          playFishingSound('snap')
          addCombatMessage({
            text: translate('fishing.escaped'),
            sender: 'local',
          })
          addChatMessage({
            text: translate('fishing.escaped'),
            sender: 'system',
          })
        } else if (outcome === 'Aborted') {
          addCombatMessage({ text: translate('fishing.reel'), sender: 'local' })
        } else if (outcome?.Caught) {
          playFishingSound('catch')
          const { item_def_id, size_cm, trophy } = outcome.Caught
          const text = catchMessage(
            getItemDef(item_def_id),
            item_def_id,
            size_cm,
            trophy
          )
          addCombatMessage({ text, sender: 'local' })
          addChatMessage({ text, sender: 'system' })
        }
      }
      break
    }

    case 'FishingError':
      reportSkillFailure(data.message, 'combat')
      break

    case 'ManaUpdate':
      manaState.set({ mana: data.mana, max_mana: data.max_mana })
      break
    case 'HungerUpdate': {
      const prev = get(hungerState)
      const band = data.state as HungerBand
      hungerState.set({
        satiation: data.satiation,
        band,
        moveMult: data.move_mult,
        attackMult: data.attack_mult,
        carryMult: data.carry_mult,
      })
      if (prev && prev.band !== band) {
        addCombatMessage({
          text: translate(HUNGER_BAND_MESSAGES[band]),
          sender: 'local',
        })
      }
      break
    }

    // Direct to the owner only: the full active list (doc/DEBUFF.md).
    case 'AbilityCooldowns':
      applyAbilityCooldowns(data.cooldowns as AbilityTimer[])
      break
    case 'BowMarkUpdate':
      updateBowMark(data.monster_id, Number(data.remaining_ms))
      break
    case 'BuffUpdate': {
      const before = get(activeBuffs)
      const next = timerSnapshot(data.buffs as AbilityTimer[])
      activeBuffs.set(next)
      if (
        next.guardian_ward &&
        (!before.guardian_ward ||
          next.guardian_ward - before.guardian_ward > 1000)
      ) {
        addCombatMessage({
          text: translate('buff.guardianWardApplied'),
          sender: 'local',
        })
      } else if (!next.guardian_ward && before.guardian_ward) {
        addCombatMessage({
          text: translate('buff.guardianWardExpired'),
          sender: 'local',
        })
      }
      if (next.radiance && !before.radiance)
        addCombatMessage({
          text: translate('buff.radianceApplied'),
          sender: 'local',
        })
      else if (!next.radiance && before.radiance)
        addCombatMessage({
          text: translate('buff.radianceExpired'),
          sender: 'local',
        })
      if (next.bow_mark && !before.bow_mark)
        addCombatMessage({
          text: translate('buff.trueAimApplied'),
          sender: 'local',
        })
      else if (!next.bow_mark && before.bow_mark)
        addCombatMessage({
          text: translate('buff.trueAimExpired'),
          sender: 'local',
        })
      break
    }
    case 'InspectionResult':
      inspectionResult.set(data.inspection as InspectionResult)
      break
    case 'AbilityRejected': {
      abilityPending.set({})
      const name = abilityDisplayName(data.ability)
      let text: string
      if (data.reason === 'not_enough_mana')
        text = translate('skillFailure.mana')
      else if (data.reason === 'out_of_range')
        text = translate('skillFailure.range')
      else if (data.reason === 'cooldown')
        text = translate('skillFailure.cooldown', { name })
      else if (data.reason === 'equipment')
        text = abilityEquipmentNotMet(data.ability)
      else text = abilityRequirementsNotMet(name)
      reportSkillFailure(text)
      break
    }
    case 'AbilityUsed':
      if (data.ability === GUARDIAN_WARD.id || data.ability === 'radiance')
        queueAbilityEffect(data as Omit<AbilityEffectEvent, 'startedAt'>)
      break
    case 'DebuffUpdate': {
      const now = Date.now()
      const prevIds = new Set(get(activeDebuffs).map((d) => d.id))
      const next: ActiveDebuff[] = (
        data.debuffs as { id: string; remaining_ms: number }[]
      ).map((d) => ({ id: d.id, until: now + Number(d.remaining_ms) }))
      activeDebuffs.set(next)
      const nextIds = new Set(next.map((d) => d.id))
      for (const id of nextIds) {
        if (!prevIds.has(id)) {
          addCombatMessage({
            text: debuffPresentation(id).applied,
            sender: 'local',
          })
        }
      }
      for (const id of prevIds) {
        if (!nextIds.has(id)) {
          addCombatMessage({
            text: debuffPresentation(id).expired,
            sender: 'local',
          })
        }
      }
      break
    }

    case 'CampfireSpawned':
    case 'CampfireAppeared':
      campfireManager.spawn(data.campfire)
      break

    case 'CampfireRemoved':
      campfireManager.remove(data.campfire_id)
      break

    case 'StallPlaced':
    case 'StallAppeared':
      stallManager.spawn(data.stall)
      break

    case 'StallRemoved':
      stallManager.remove(data.stall_id)
      if (get(openStall)?.stall_id === data.stall_id) closeStallPanel()
      break

    case 'StallSignChanged':
      stallManager.setSign(data.stall_id, data.sign)
      break

    case 'StallState':
      openStall.set(data)
      break

    case 'TipHatPlaced':
    case 'TipHatAppeared':
      tipHatManager.spawn(data.tip_hat)
      break

    case 'TipHatRemoved':
      tipHatManager.remove(data.tip_hat_id)
      break

    case 'MealPlaced':
    case 'MealAppeared':
      mealManager.spawn(data.meal)
      break

    case 'MealEaten':
      mealManager.markEaten(data.meal_id)
      break

    case 'MealRemoved':
      mealManager.remove(data.meal_id)
      break

    case 'GrillStarted':
      grilling.set(true)
      break

    case 'GrillEnded':
      grilling.set(false)
      if (data.grilled_item_def_id == null) {
        addCombatMessage({
          text: translate('system.grillInterrupted'),
          sender: 'local',
        })
      }
      break

    case 'DungeonReset':
      playDungeonSound('reset')
      break
  }
}

const HUNGER_BAND_MESSAGES: Record<HungerBand, MessageKey> = {
  Normal: 'hunger.normalMessage',
  Hungry: 'hunger.hungryMessage',
  Weak: 'hunger.weakMessage',
}

const DAGGER_REJECTION_MESSAGES: Record<string, MessageKey> = {
  cooldown: 'combat.skillCoolingDown',
  attack_cooldown: 'combat.attackCooldown',
  invalid_target: 'combat.invalid_target',
  out_of_range: 'combat.out_of_range',
  attacker_dead: 'combat.attackerDead',
  busy: 'combat.busy',
}

const ATTACK_REJECTION_MESSAGES: Record<string, MessageKey> = {
  invalid_target: 'combat.invalid_target',
  out_of_range: 'combat.out_of_range',
  attacker_dead: 'combat.attackerDead',
  out_of_ammo: 'combat.outOfAmmo',
}
