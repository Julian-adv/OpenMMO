export interface ItemHoldings {
  inventory: number
  npc_inventory: number
  storage: number
  ground: number
}

export interface RareDropItem {
  item_def_id: string
  name: string
  world_drop: { chance: number, low_level_chance: number | null, low_level_max_level: number | null } | null
  monster_drops: { monster_type: string, name: string, chance: number }[]
  kills: number
  expected_kill_drops: number
  kill_drops: number
  other_drops: number
  holdings: ItemHoldings
}

export interface RareDrops {
  until: number
  collection_started_at: number
  total_kills: number
  items: RareDropItem[]
}

const count = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) >= 0
const chance = (value: unknown): value is number => typeof value === 'number' && value >= 0 && value <= 1
const text = (value: unknown): value is string => typeof value === 'string' && value.length > 0

function validItem(item: RareDropItem): boolean {
  const world = item.world_drop
  const holdings = item.holdings
  return text(item.item_def_id) && text(item.name)
    && (world === null || (chance(world.chance)
      && (world.low_level_chance === null || chance(world.low_level_chance))
      && (world.low_level_max_level === null || count(world.low_level_max_level))))
    && Array.isArray(item.monster_drops)
    && item.monster_drops.every((drop) => drop && text(drop.monster_type) && text(drop.name) && chance(drop.chance))
    && count(item.kills) && count(item.kill_drops) && count(item.other_drops)
    && typeof item.expected_kill_drops === 'number' && Number.isFinite(item.expected_kill_drops) && item.expected_kill_drops >= 0
    && !!holdings && [holdings.inventory, holdings.npc_inventory, holdings.storage, holdings.ground].every(count)
}

export function parseRareDrops(value: unknown): RareDrops {
  const data = value as RareDrops
  if (!data || typeof data !== 'object' || !count(data.until) || !count(data.collection_started_at)
    || !count(data.total_kills) || !Array.isArray(data.items)
    || !data.items.every((item) => item && typeof item === 'object' && validItem(item) && item.kills <= data.total_kills)) {
    throw new Error('Invalid rare drops response')
  }
  return data
}

export const formatChance = (value: number) =>
  `${(value * 100).toLocaleString('ko-KR', { maximumFractionDigits: 3 })}%`

/** Per-kill rates are tiny, so they read per 10,000 kills. */
export const perTenThousandKills = (drops: number, kills: number) =>
  kills === 0 ? null : (drops / kills * 10000).toLocaleString('ko-KR', { maximumFractionDigits: 2 })

export const totalHoldings = (holdings: ItemHoldings) =>
  holdings.inventory + holdings.npc_inventory + holdings.storage + holdings.ground

export function describeChances(item: RareDropItem): string[] {
  const lines: string[] = []
  const world = item.world_drop
  if (world) {
    if (world.low_level_chance !== null && world.low_level_max_level !== null) {
      lines.push(`몬스터 Lv${world.low_level_max_level + 1}+ ${formatChance(world.chance)} · Lv${world.low_level_max_level} 이하 ${formatChance(world.low_level_chance)}`)
    } else {
      lines.push(`모든 몬스터 ${formatChance(world.chance)}`)
    }
    lines.push(`상자·통 ${formatChance(world.chance)}`)
  }
  const byChance = new Map<number, string[]>()
  for (const drop of item.monster_drops) byChance.set(drop.chance, [...(byChance.get(drop.chance) ?? []), drop.name])
  for (const [value, names] of byChance) lines.push(`${names.join('·')} ${formatChance(value)}`)
  return lines
}
