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
}

const monsterDefs = monstersJson as Record<string, MonsterDefinition>

export function getMonsterDef(type: string): MonsterDefinition | undefined {
  return monsterDefs[type]
}

export default monsterDefs
