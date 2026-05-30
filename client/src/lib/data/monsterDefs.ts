import monstersJson from '../../../../data/monsters.json'

export interface MonsterDefinition {
  id: string
  name: string
  model: string
  health: number
  walkSpeed: number
  runSpeed: number
  attackRange: number
  chaseRange: number
  attackCooldown: number
  fleeHealthRatio: number
  fleeChance: number
  returnChance: number
  damageRoll: string
  hitThreshold: number
  animIdle: string
  animWalk: string
  animRun: string
  animAttack: string
  animHit: string
  animDie: string
  animDead: string
  /** Optional weapon item id, or legacy model path relative to /models/. */
  weapon?: string
  /** Chance from 0-1 that the weapon is dropped on death. */
  weaponDropChance?: number
  /** Skeleton bone name the weapon is parented to (e.g. 'RightHand'). */
  weaponBone?: string
}

const monsterDefs = monstersJson as Record<string, MonsterDefinition>

export function getMonsterDef(type: string): MonsterDefinition | undefined {
  return monsterDefs[type]
}

export default monsterDefs
