import type { MonsterData } from '../types/Monster'
import type { WallDirection } from '../utils/house-geometry'

export type Position = {
  x: number
  y: number
  z: number
}

export type CharacterClass =
  | 'warrior'
  | 'knight'
  | 'thief'
  | 'merchant'
  | 'guard'

export type ServerPlayer = {
  id: string
  name: string
  position: Position
  rotation: number
  level: number
  health: number
  max_health: number
  class: CharacterClass
  is_npc: boolean
  torch_on: boolean
  furniture_type?: string
}

export type ServerMonster = {
  id: string
  monster_type: string
  position: Position
  rotation: number
  state: MonsterData['state']
  owner_id?: string
  health: number
  max_health: number
}

export type AccountCharacter = {
  id: number
  name: string
  created_at: number
  level: number
  xp: number
  max_hp: number
  attributes: CharacterAttributes
  class: CharacterClass
}

export type CharacterAttributes = {
  str: number
  dex: number
  con: number
  int: number
  wis: number
  cha: number
  guard: number
}

export type CharacterRollResult = {
  attributes: CharacterAttributes
  maxHp: number
}

export type RollCharacterStatsResult =
  | {
      ok: true
      attributes: CharacterAttributes
      maxHp: number
    }
  | {
      ok: false
      message: string
    }

// Serde externally tagged enum shapes
export type ClientMessage =
  | {
      Authenticate: {
        account_name: string
        password_hash: string
        create_account: boolean
      }
    }
  | {
      CreateCharacter: {
        character_name: string
        character_class: CharacterClass
      }
    }
  | { DeleteCharacter: { character_id: number } }
  | 'RollCharacterStats'
  | { EnterGame: { character_id: number } }
  | {
      PlayerMove: { position: Position; rotation: number; floor_level: number }
    }
  | { ChatMessage: { message: string } }
  | {
      RequestSpawnMonster: {
        monster_type: string
        position: Position
        rotation: number
      }
    }
  | {
      MonsterMove: {
        monster_id: string
        position: Position
        rotation: number
        state: MonsterData['state']
        target_position: Position
      }
    }
  | { PlayerAttack: { monster_id: string } }
  | { MonsterAttack: { monster_id: string; target_player_id: string } }
  | 'RequestRespawn'
  | { DebugTeleport: { position: Position } }
  | { TorchToggle: { enabled: boolean } }
  | {
      ToggleDoor: {
        house_id: string
        room_index: number
        wall_dir: WallDirection
        segment_index: number
      }
    }
  | { InteractFurniture: { furniture_type: string; furniture_id: number } }
  | 'StopInteraction'
  | 'Heartbeat'

export type AuthSuccessPayload = {
  accountName: string
  characters: AccountCharacter[]
}
