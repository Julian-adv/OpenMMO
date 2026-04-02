import { get } from 'svelte/store'
import {
  gameStore,
  updatePlayer,
  addChatMessage,
  addCombatMessage,
  addChatBubble,
  resetGameStore,
} from '../stores/gameStore'
import type { LocalPlayer, RemotePlayer } from '../stores/gameStore'
import { Vector3 } from 'three'
import { remotePlayerManager } from '../managers/remotePlayerManager'
import { monsterManager } from '../managers/monsterManager'
import { housingManager } from '../managers/housingManager'
import { furnitureManager } from '../managers/furnitureManager'
import type { MonsterData } from '../types/Monster'
import { requestCameraReset } from '../stores/cameraStore'
import { setServerGameTime } from '../stores/timeStore'
import type { NetworkEvent } from './networkEvents'
import type {
  AccountCharacter,
  AuthSuccessPayload,
  CharacterAttributes,
  CharacterRollResult,
  ServerMonster,
  ServerPlayer,
} from './networkTypes'

/** Resolve furniture interaction for a remote player: find nearest placement, snap position/rotation. */
async function applyFurnitureInteraction(
  playerId: string,
  furnitureType: string,
  wx: number,
  wz: number
) {
  await furnitureManager.fetchCatalog()
  const def = furnitureManager.getCatalogEntry(furnitureType)
  const anim = def?.interaction ?? furnitureType
  const offsetY = def?.interactOffset?.y ?? 0
  const placement = await furnitureManager.findNearestPlacementAsync(
    furnitureType,
    wx,
    wz
  )
  const pos = placement
    ? { x: placement.x, y: placement.y, z: placement.z }
    : undefined
  const rot = placement ? placement.rotation : undefined
  remotePlayerManager.handleInteraction(playerId, anim, offsetY, pos, rot)
}

export type MessageEvents = {
  authSuccess: NetworkEvent<(payload: AuthSuccessPayload) => void>
  authError: NetworkEvent<(message: string) => void>
  joinSuccess: NetworkEvent<() => void>
  characterCreated: NetworkEvent<(character: AccountCharacter) => void>
  characterStatsRolled: NetworkEvent<(result: CharacterRollResult) => void>
  characterDeleted: NetworkEvent<(characterId: number) => void>
  characterError: NetworkEvent<(message: string) => void>
  kicked: NetworkEvent<(reason: string) => void>
  playerRespawned: NetworkEvent<(playerId: string) => void>
  interactionRejected: NetworkEvent<(reason: string) => void>
}

export function handleServerMessage(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  raw: any,
  events: MessageEvents,
  disconnect: () => void
) {
  if (typeof raw === 'string') {
    return
  }

  const type = Object.keys(raw)[0]
  const data = raw[type]

  switch (type) {
    case 'AuthSuccess': {
      const characters = (data.characters as AccountCharacter[]) ?? []
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
      const serverPlayer: ServerPlayer = data.player
      console.log('Join successful, received player data:', serverPlayer)
      const playerPosition = new Vector3(
        serverPlayer.position.x,
        serverPlayer.position.y,
        serverPlayer.position.z
      )
      const player: LocalPlayer = {
        ...serverPlayer,
        position: playerPosition,
        rotation: serverPlayer.rotation ?? 0,
        maxHealth: serverPlayer.max_health,
        characterClass: serverPlayer.class,
      }
      gameStore.update((state) => ({
        ...state,
        currentPlayer: player,
      }))
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

    case 'CharacterError': {
      events.characterError.emit(data.message)
      break
    }

    case 'PlayerJoined': {
      const serverPlayer: ServerPlayer = data.player
      const playerPosition = new Vector3(
        serverPlayer.position.x,
        serverPlayer.position.y,
        serverPlayer.position.z
      )
      const player: LocalPlayer = {
        ...serverPlayer,
        position: playerPosition,
        rotation: serverPlayer.rotation ?? 0,
        maxHealth: serverPlayer.max_health,
        characterClass: serverPlayer.class,
      }
      const remotePlayer: RemotePlayer = {
        id: serverPlayer.id,
        name: serverPlayer.name,
        level: serverPlayer.level,
        health: serverPlayer.health,
        maxHealth: serverPlayer.max_health,
        characterClass: serverPlayer.class,
        torchOn: serverPlayer.torch_on,
      }
      let joinedName: string | null = null
      gameStore.update((state) => {
        if (!state.currentPlayer) {
          console.log('Setting current player from PlayerJoined:', player)
          return { ...state, currentPlayer: player }
        } else if (serverPlayer.id !== state.currentPlayer.id) {
          remotePlayerManager.initPlayer(
            serverPlayer.id,
            serverPlayer.position,
            serverPlayer.rotation
          )
          if (serverPlayer.furniture_type) {
            applyFurnitureInteraction(
              serverPlayer.id,
              serverPlayer.furniture_type,
              serverPlayer.position.x,
              serverPlayer.position.z
            )
          }
          state.otherPlayers.set(serverPlayer.id, remotePlayer)
          joinedName = serverPlayer.name
        }
        return state
      })
      if (joinedName) {
        addChatMessage({
          text: `${joinedName} joined the game`,
          sender: 'system',
        })
      }
      break
    }

    case 'PlayerLeft': {
      let leftName: string | null = null
      gameStore.update((state) => {
        const player = state.otherPlayers.get(data.player_id)
        remotePlayerManager.removePlayer(data.player_id)
        if (player) {
          state.otherPlayers.delete(data.player_id)
          leftName = player.name
        }
        return state
      })
      if (leftName) {
        addChatMessage({ text: `${leftName} left the game`, sender: 'system' })
      }
      break
    }

    case 'PlayerMoved': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      remotePlayerManager.setTargetPosition(
        data.player_id,
        {
          x: data.position.x,
          y: data.position.y,
          z: data.position.z,
        },
        data.rotation
      )
      break
    }

    case 'PlayerTeleported': {
      const state = get(gameStore)
      if (state.currentPlayer && state.currentPlayer.id === data.player_id) {
        state.currentPlayer.position.set(
          data.position.x,
          data.position.y,
          data.position.z
        )
        requestCameraReset()
        break
      }
      remotePlayerManager.teleportPlayer(
        data.player_id,
        data.position,
        data.rotation
      )
      break
    }

    case 'ChatMessage': {
      const state = get(gameStore)
      const isLocal = state.currentPlayer?.id === data.player_id
      const playerName = isLocal
        ? state.currentPlayer?.name
        : (state.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
      addChatMessage({
        text: data.message,
        sender: isLocal ? 'local' : 'remote',
        name: playerName,
      })
      addChatBubble(data.player_id, data.message)
      break
    }

    case 'GameState':
      gameStore.update((state) => {
        state.otherPlayers.clear()
        remotePlayerManager.reset()
        Object.values(data.players as Record<string, ServerPlayer>).forEach(
          (serverPlayer) => {
            if (serverPlayer.id !== state.currentPlayer?.id) {
              const player: RemotePlayer = {
                id: serverPlayer.id,
                name: serverPlayer.name,
                level: serverPlayer.level,
                health: serverPlayer.health,
                maxHealth: serverPlayer.max_health,
                characterClass: serverPlayer.class,
                torchOn: serverPlayer.torch_on,
              }
              remotePlayerManager.initPlayer(
                serverPlayer.id,
                serverPlayer.position,
                serverPlayer.rotation
              )
              if (serverPlayer.furniture_type) {
                applyFurnitureInteraction(
                  serverPlayer.id,
                  serverPlayer.furniture_type,
                  serverPlayer.position.x,
                  serverPlayer.position.z
                )
              }
              state.otherPlayers.set(serverPlayer.id, player)
            }
          }
        )
        return state
      })

      monsterManager.reset()
      if (data.monsters) {
        Object.values(data.monsters as Record<string, ServerMonster>).forEach(
          (monster) => {
            monsterManager.spawnWithId(
              monster.id,
              monster.monster_type as MonsterData['type'],
              monster.position,
              monster.owner_id,
              monster.health,
              monster.max_health
            )
          }
        )
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

    case 'MonsterSpawned': {
      const monster: ServerMonster = data.monster
      monsterManager.spawnWithId(
        monster.id,
        monster.monster_type as MonsterData['type'],
        monster.position,
        monster.owner_id,
        monster.health,
        monster.max_health
      )
      break
    }

    case 'SpawnMonsterRequest': {
      // Server asks us to spawn a monster within the given rectangular area.
      const x = data.min_x + Math.random() * (data.max_x - data.min_x)
      const z = data.min_z + Math.random() * (data.max_z - data.min_z)
      const rotation = Math.random() * Math.PI * 2
      monsterManager.requestSpawnFromServer(
        data.monster_type,
        { x, y: 0, z },
        rotation
      )
      break
    }

    case 'NoSpawnZones':
      break

    case 'MonsterAssigned': {
      const assigned: ServerMonster = data.monster
      monsterManager.spawnWithId(
        assigned.id,
        assigned.monster_type as MonsterData['type'],
        assigned.position,
        assigned.owner_id,
        assigned.health,
        assigned.max_health
      )
      break
    }

    case 'MonsterMoved':
      monsterManager.updateMonsterFromNetwork(
        data.monster_id,
        data.position,
        data.rotation,
        data.state,
        data.target_position
      )
      break

    case 'MonsterRemoved':
      monsterManager.remove(data.monster_id)
      break

    case 'MonsterDead':
      monsterManager.handleMonsterDead(data.monster_id)
      break

    case 'PlayerAttacked': {
      remotePlayerManager.handleAttack(data.player_id)

      const gameState = get(gameStore)
      const isLocalAttacker = gameState.currentPlayer?.id === data.player_id
      const attackerName = isLocalAttacker
        ? 'You'
        : gameState.otherPlayers.get(data.player_id)?.name || 'Unknown'

      addCombatMessage({
        text: data.hit
          ? `rolled ${data.roll}: HIT for ${data.damage} damage!`
          : `rolled ${data.roll}: MISSED!`,
        sender: isLocalAttacker ? 'local' : 'remote',
        name: attackerName,
        hit: data.hit,
      })

      monsterManager.handleMonsterAttacked(
        data.monster_id,
        data.player_id,
        data.hit,
        data.damage
      )
      break
    }

    case 'MonsterAttackedPlayer': {
      const gameState = get(gameStore)
      const isCurrentPlayer = gameState.currentPlayer?.id === data.player_id

      let damageInfo = undefined
      if (isCurrentPlayer) {
        const prevTrigger =
          gameState.currentPlayer?.lastDamageInfo?.trigger ?? 0
        damageInfo = {
          damage: data.damage,
          hit: data.hit,
          trigger: prevTrigger + 1,
        }
      }

      updatePlayer(data.player_id, {
        health: data.current_health,
        ...(isCurrentPlayer ? { lastDamageInfo: damageInfo } : {}),
      })

      const monsterTargetName = isCurrentPlayer
        ? 'You'
        : (gameState.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
      addCombatMessage({
        text: data.hit
          ? `rolled ${data.roll}: HIT ${monsterTargetName} for ${data.damage} damage!`
          : `rolled ${data.roll}: MISSED!`,
        sender: 'system',
        name: 'Monster',
        hit: data.hit,
      })
      break
    }

    case 'PlayerDead': {
      console.log('Player dead:', data.player_id)
      const gameState = get(gameStore)
      const isDeadCurrentPlayer = gameState.currentPlayer?.id === data.player_id
      const deadPlayerName = isDeadCurrentPlayer
        ? 'You'
        : (gameState.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
      addCombatMessage({
        text: `${deadPlayerName === 'You' ? 'You have' : deadPlayerName + ' has'} been slain!`,
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
      remotePlayerManager.reset()
      disconnect()
      break
    }

    case 'PlayerRespawned': {
      const serverPlayer: ServerPlayer = data.player
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
        requestCameraReset()
        addChatMessage({ text: 'You have respawned.', sender: 'system' })
      } else {
        updatePlayer(serverPlayer.id, {
          health: serverPlayer.health,
          maxHealth: serverPlayer.max_health,
        })
        addChatMessage({
          text: `${serverPlayer.name} has respawned.`,
          sender: 'system',
        })
        remotePlayerManager.handleRespawn(
          serverPlayer.id,
          serverPlayer.position,
          serverPlayer.rotation
        )
      }
      events.playerRespawned.emit(serverPlayer.id)
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

    case 'PlayerInteractionChanged': {
      const state = get(gameStore)
      if (state.currentPlayer?.id === data.player_id) {
        break
      }
      const ft: string | null = data.furniture_type ?? null
      if (ft) {
        const rp = remotePlayerManager.players.get(data.player_id)
        const wx = rp?.position.x ?? 0
        const wz = rp?.position.z ?? 0
        applyFurnitureInteraction(data.player_id, ft, wx, wz)
      } else {
        remotePlayerManager.handleStopInteraction(data.player_id)
      }
      break
    }

    case 'InteractionRejected':
      events.interactionRejected.emit(data.reason)
      break

    case 'HouseSpawned':
      housingManager.handleRemoteHouseSpawned(data.house)
      break

    case 'HouseUpdated':
      housingManager.handleRemoteHouseSpawned(data.house)
      break

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

      updatePlayer(data.player_id, {
        level: data.new_level,
        totalXp: newTotalXp,
        health: data.current_hp,
        maxHealth: data.max_hp,
        ...(isCurrentPlayer ? { lastRegenInfo: regenInfo } : {}),
      })
      if (data.xp_amount > 0) {
        addCombatMessage({
          text: `You gained ${data.xp_amount} XP.`,
          sender: 'local',
        })
      } else if (previousLevel !== null) {
        if (xpLost > 0) {
          addCombatMessage({
            text: `Death penalty: You lost ${xpLost} XP.`,
            sender: 'local',
          })
        } else {
          addCombatMessage({ text: 'Death penalty applied.', sender: 'local' })
        }
      }
      if (data.leveled_up) {
        addCombatMessage({
          text: `Level up! You are now level ${data.new_level}.`,
          sender: 'local',
        })
      } else if (previousLevel !== null && data.new_level < previousLevel) {
        addCombatMessage({
          text: `Level down. You are now level ${data.new_level}.`,
          sender: 'local',
        })
      }
      break
    }
  }
}
