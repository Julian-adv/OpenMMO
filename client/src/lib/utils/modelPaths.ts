import type { CharacterClass, Gender } from '../network/networkTypes'

export const MODULAR_MALE_DIRECTORY = '/models/characters/modular_male'
export const MODULAR_MALE_MODEL_PATH = `${MODULAR_MALE_DIRECTORY}/base.glb`

export const MODULAR_FEMALE_DIRECTORY = '/models/characters/modular_female'
export const MODULAR_FEMALE_MODEL_PATH = `${MODULAR_FEMALE_DIRECTORY}/base.glb`

export const MODULAR_VALKYRIE_MODEL_PATH = `${MODULAR_FEMALE_DIRECTORY}/valkyrie.glb`

export function isModularFemaleModel(path: string): boolean {
  return (
    path === MODULAR_FEMALE_MODEL_PATH || path === MODULAR_VALKYRIE_MODEL_PATH
  )
}

export function isModularModel(path: string): boolean {
  return path === MODULAR_MALE_MODEL_PATH || isModularFemaleModel(path)
}

export const KNIGHT_CHARACTER_MODEL_PATH = '/models/characters/knight.glb'
export const FEMALE_KNIGHT_CHARACTER_MODEL_PATH =
  '/models/characters/female_knight.glb'
export const ROGUE_CHARACTER_MODEL_PATH = '/models/characters/rogue.glb'
export const FEMALE_ROGUE_CHARACTER_MODEL_PATH =
  '/models/characters/female_rogue.glb'
export const MERCHANT_CHARACTER_MODEL_PATH = '/models/characters/npc_woman.glb'
export const BARBARIAN_CHARACTER_MODEL_PATH = '/models/characters/barbarian.glb'
export const FEMALE_BARBARIAN_CHARACTER_MODEL_PATH =
  '/models/characters/female_barbarian.glb'
export const GUARD_CHARACTER_MODEL_PATH = '/models/characters/guard.glb'
export const CAVEMAN_CHARACTER_MODEL_PATH = '/models/characters/caveman.glb'
export const CAVEWOMAN_CHARACTER_MODEL_PATH = '/models/characters/cavewoman.glb'
export const VALKYRIE_CHARACTER_MODEL_PATH = '/models/characters/valkyrie.glb'
export const RANGER_CHARACTER_MODEL_PATH = '/models/characters/ranger.glb'
export const PRIEST_CHARACTER_MODEL_PATH = '/models/characters/priest.glb'
export const FEMALE_PRIEST_CHARACTER_MODEL_PATH =
  '/models/characters/female_priest.glb'
export const FEMALE_BARD_CHARACTER_MODEL_PATH =
  '/models/characters/female_bard.glb'
export const NIGHT_MERCHANT_CHARACTER_MODEL_PATH =
  '/models/characters/night_merchant.glb'
export const MAID_CHARACTER_MODEL_PATH = '/models/characters/maid.glb'
export const PINK_MAID_CHARACTER_MODEL_PATH = '/models/characters/pink_maid.glb'
export const STEWARD_CHARACTER_MODEL_PATH = '/models/characters/steward.glb'
export const ESTATE_ARCHITECT_CHARACTER_MODEL_PATH =
  '/models/characters/estate_architect.glb'
export const GRIDA_CHARACTER_MODEL_PATH = '/models/characters/grida.glb'
export const TOBIN_CHARACTER_MODEL_PATH = '/models/characters/tobin.glb'
export const YSOLDE_CHARACTER_MODEL_PATH = '/models/characters/ysolde.glb'

export const CHARACTER_ANIMATION_PACK_PATHS = {
  locomotion: '/models/animations/locomotion.glb',
  combatMelee: '/models/animations/combat_melee.glb',
  combatRanged: '/models/animations/combat_ranged.glb',
  social: '/models/animations/social.glb',
  enchantArmor: '/models/animations/enchant_armor.glb',
  offhand: '/models/animations/offhand.glb',
  fishing: '/models/animations/fishing.glb',
} as const

export function getWeaponModelPath(worldModel: string): string {
  return `/models/${worldModel}`
}

/** Object catalog paths are relative to /models/. */
export function getObjectModelPath(model: string): string {
  return `/models/${model}`
}

const CLASS_GENDER_MODELS: Partial<
  Record<CharacterClass, Partial<Record<Gender, string>>>
> = {
  knight: {
    male: KNIGHT_CHARACTER_MODEL_PATH,
    female: FEMALE_KNIGHT_CHARACTER_MODEL_PATH,
  },
  barbarian: {
    male: BARBARIAN_CHARACTER_MODEL_PATH,
    female: FEMALE_BARBARIAN_CHARACTER_MODEL_PATH,
  },
  rogue: {
    male: ROGUE_CHARACTER_MODEL_PATH,
    female: FEMALE_ROGUE_CHARACTER_MODEL_PATH,
  },
  caveman: {
    male: CAVEMAN_CHARACTER_MODEL_PATH,
    female: CAVEWOMAN_CHARACTER_MODEL_PATH,
  },
  valkyrie: { female: VALKYRIE_CHARACTER_MODEL_PATH },
  ranger: { male: RANGER_CHARACTER_MODEL_PATH },
  priest: {
    male: PRIEST_CHARACTER_MODEL_PATH,
    female: FEMALE_PRIEST_CHARACTER_MODEL_PATH,
  },
  bard: {
    female: FEMALE_BARD_CHARACTER_MODEL_PATH,
    male: MODULAR_MALE_MODEL_PATH,
  },
  merchant: { female: MERCHANT_CHARACTER_MODEL_PATH },
  guard: { male: GUARD_CHARACTER_MODEL_PATH },
  maid: { female: MAID_CHARACTER_MODEL_PATH },
}

/** Named NPC models override their class model. */
const NPC_MODEL_OVERRIDES: Record<string, string> = {
  Wick: NIGHT_MERCHANT_CHARACTER_MODEL_PATH,
  Cocoly: PINK_MAID_CHARACTER_MODEL_PATH,
  Aldwin: STEWARD_CHARACTER_MODEL_PATH,
  Rowan: ESTATE_ARCHITECT_CHARACTER_MODEL_PATH,
  Grida: GRIDA_CHARACTER_MODEL_PATH,
  Tobin: TOBIN_CHARACTER_MODEL_PATH,
  Ysolde: YSOLDE_CHARACTER_MODEL_PATH,
}

export function getNpcModelPath(npcName: string): string | undefined {
  return NPC_MODEL_OVERRIDES[npcName]
}

/** Models whose own baked clips win over the shared animation packs. */
const OWN_CLIP_MODEL_PATHS = new Set([YSOLDE_CHARACTER_MODEL_PATH])

export function usesOwnClips(path: string): boolean {
  return OWN_CLIP_MODEL_PATHS.has(path)
}

const NPC_CLASSES: CharacterClass[] = ['merchant', 'guard', 'maid']
export const PLAYER_CLASSES = (
  Object.keys(CLASS_GENDER_MODELS) as CharacterClass[]
).filter((cls) => !NPC_CLASSES.includes(cls))

export function getAvailableGenders(characterClass: CharacterClass): Gender[] {
  const genders = CLASS_GENDER_MODELS[characterClass]
  if (!genders) return ['male', 'female']
  return Object.keys(genders) as Gender[]
}

export function getCharacterModelPath(
  characterClass: CharacterClass,
  gender?: Gender,
  isNpc = false
): string {
  const genders = CLASS_GENDER_MODELS[characterClass]
  const effectiveGender = gender ?? getAvailableGenders(characterClass)[0]
  if (
    !isNpc &&
    PLAYER_CLASSES.includes(characterClass) &&
    effectiveGender === 'male' &&
    genders?.male
  )
    return MODULAR_MALE_MODEL_PATH
  if (
    !isNpc &&
    (characterClass === 'knight' ||
      characterClass === 'barbarian' ||
      characterClass === 'rogue') &&
    effectiveGender === 'female'
  )
    return MODULAR_FEMALE_MODEL_PATH
  if (!isNpc && characterClass === 'valkyrie' && effectiveGender === 'female')
    return MODULAR_VALKYRIE_MODEL_PATH
  if (genders) {
    if (gender && genders[gender]) return genders[gender]
    return Object.values(genders)[0]
  }
  return KNIGHT_CHARACTER_MODEL_PATH
}
