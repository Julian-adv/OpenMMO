import monstersJson from '../../../../data/monsters.json'

export interface MonsterDefinition {
  id: string
  name: string
  model: string
  health?: number
  level: number
  guard: number
  attackBonus?: number
  walkSpeed: number
  runSpeed: number
  attackRange: number
  chaseRange: number
  attackCooldown: number
  attackImpactDelay: number
  attackDamageTextDelay: number
  behavior: string
  damageRoll?: string
  animIdle: string
  animWalk: string
  animRun: string
  animAttack: string
  animAttackIdle?: string
  animHit: string
  animDie: string
  animDead: string
  material?: string
  /**
   * When true (default), a killing blow plays the hit reaction before the death
   * clip. Set false for monsters whose hit clip looks awkward as a death lead-in
   * (e.g. scp939's long additive stagger).
   */
  deathPlaysHit?: boolean
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
