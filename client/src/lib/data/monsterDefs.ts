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
  damageRoll: string
  hitThreshold: number
}

const monsterDefs: Record<string, MonsterDefinition> = {
  scp939: {
    id: 'scp939',
    name: 'SCP-939',
    model: 'scp939.glb',
    health: 10,
    walkSpeed: 1,
    runSpeed: 8,
    attackRange: 2,
    chaseRange: 25,
    attackCooldown: 1500,
    damageRoll: '1d6',
    hitThreshold: 10,
  },
}

export function getMonsterDef(type: string): MonsterDefinition | undefined {
  return monsterDefs[type]
}

export default monsterDefs
