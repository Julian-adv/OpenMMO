import { get, writable } from 'svelte/store'
import { assetUrl } from '../utils/assetUrl'
import {
  DEFAULT_MATERIAL_HIT_SOUND_URL,
  DEFAULT_MATERIAL_MISS_SOUND_URL,
  getAllMaterialHitSoundUrls,
  getAllMaterialMissSoundUrls,
} from '../data/materialImpactSounds'
import monsterDefs from '../data/monsterDefs'
import type { Gender } from '../network/networkTypes'
import type { AbilityId } from '../data/abilities'

const MONSTER_DEATH_VOLUME = 0.5
const MONSTER_DEATH_POOL_SIZE = 3
const MONSTER_SOUND_VOLUME = 0.55
const MONSTER_SOUND_POOL_SIZE = 3
const SWORD_HIT_VOLUME = 0.55
const SWORD_MISS_VOLUME = 0.5
const SWORD_HIT_POOL_SIZE = 4
const SWORD_MISS_POOL_SIZE = 4

interface SoundSpec {
  url: string
  volume: number
  pool: number
}
type SoundTable = Readonly<Record<string, SoundSpec>>

// The reel fires on every reel-stance engage, so it gets a deeper pool; the
// rest are one-shot moments.
const FISHING_SOUNDS = {
  cast: { url: '/sounds/fishing-cast.ogg', volume: 0.45, pool: 2 },
  splash: { url: '/sounds/fishing-splash.ogg', volume: 0.5, pool: 2 },
  plop: { url: '/sounds/fishing-plop.ogg', volume: 0.6, pool: 2 },
  reel: { url: '/sounds/fishing-reel.ogg', volume: 0.4, pool: 4 },
  snap: { url: '/sounds/fishing-snap.ogg', volume: 0.5, pool: 2 },
  catch: { url: '/sounds/fishing-catch.ogg', volume: 0.45, pool: 2 },
} as const
export type FishingSound = keyof typeof FISHING_SOUNDS

const PROP_SOUNDS = {
  break: { url: '/sounds/crate-break.ogg', volume: 0.4, pool: 2 },
  chestOpen: { url: '/sounds/chest-open.ogg', volume: 0.5, pool: 2 },
  coinSpill: { url: '/sounds/coin-spill.ogg', volume: 0.5, pool: 2 },
  coinJackpot: { url: '/sounds/coin-jackpot.ogg', volume: 0.5, pool: 1 },
  keyDrop: { url: '/sounds/key-drop.ogg', volume: 0.5, pool: 1 },
  pickaxe: { url: '/sounds/pickaxe-ore.ogg', volume: 0.5, pool: 3 },
} as const
export type PropSound = keyof typeof PROP_SOUNDS

// Voice on taking damage. Pool 1 each: only one cry ever plays at a time.
const PLAYER_HURT_SOUNDS: Record<Gender, SoundSpec> = {
  female: { url: '/sounds/player-hurt-female.ogg', volume: 0.5, pool: 1 },
  male: { url: '/sounds/player-hurt-male.ogg', volume: 0.5, pool: 1 },
}

// Death cry. Pool 2: two players can fall within a breath of each other.
const PLAYER_DEATH_SOUNDS: Record<Gender, SoundSpec> = {
  female: { url: '/sounds/player-death-female.ogg', volume: 0.5, pool: 2 },
  male: { url: '/sounds/player-death-male.ogg', volume: 0.5, pool: 2 },
}

// Bow draw and loose. Pool 2: the next shot can start while the last is
// still ringing.
const BOW_SOUNDS = {
  draw: { url: '/sounds/bow-draw.ogg', volume: 0.4, pool: 2 },
  release: { url: '/sounds/bow-release.ogg', volume: 0.5, pool: 2 },
} as const
export type BowSound = keyof typeof BOW_SOUNDS

const DUNGEON_SOUNDS = {
  reset: { url: '/sounds/dungeon-roar.ogg', volume: 0.5, pool: 1 },
  drip: { url: '/sounds/dungeon-drip.ogg', volume: 0.7, pool: 4 },
  drip2: { url: '/sounds/dungeon-drip-2.ogg', volume: 0.7, pool: 4 },
} as const
export type DungeonSound = keyof typeof DUNGEON_SOUNDS

const DUNGEON_DRIP_SOUNDS = [DUNGEON_SOUNDS.drip, DUNGEON_SOUNDS.drip2]

const CART_SOUNDS = {
  roll: { url: '/sounds/cart-roll.ogg', volume: 0.5, pool: 2 },
  crash: { url: '/sounds/cart-crash.ogg', volume: 0.5, pool: 2 },
} as const
/** From the crash file's start to its peak. */
const CART_CRASH_LEAD_S = 0.04
const CART_SOUND_RANGE = 16

const ABILITY_SOUNDS: Partial<Record<AbilityId, SoundSpec>> = {
  guardian_ward: {
    url: '/sounds/guardian-ward.ogg',
    volume: 0.55,
    pool: 3,
  },
  healing_prayer: {
    url: '/sounds/healing-prayer.ogg',
    volume: 0.25,
    pool: 3,
  },
}

// Primal Roar's cry. Loaded on first roar: only Cavemen cast it.
const PRIMAL_ROAR_SOUNDS: Record<Gender, SoundSpec> = {
  female: { url: '/sounds/primal-roar-female.ogg', volume: 0.6, pool: 2 },
  male: { url: '/sounds/primal-roar-male.ogg', volume: 0.6, pool: 2 },
}

// Pool 2: party members can level on the same kill.
const LEVEL_UP_SOUND: SoundSpec = {
  url: '/sounds/level-up.ogg',
  volume: 0.5,
  pool: 2,
}

const STORAGE_KEY_VOLUME = 'onlinerpg_sfxVolume'
const STORAGE_KEY_MUTED = 'onlinerpg_sfxMuted'
const DEFAULT_SFX_VOLUME = 0.5

// Node ≥22 exposes a localStorage global whose methods are unusable without
// --localstorage-file, so feature-test the method, not the object.
const storage =
  typeof localStorage !== 'undefined' &&
  typeof localStorage.getItem === 'function'
    ? localStorage
    : null

function loadSfxVolume(): number {
  const saved = storage?.getItem(STORAGE_KEY_VOLUME)
  if (saved != null) {
    const v = parseFloat(saved)
    if (!isNaN(v)) return Math.max(0, Math.min(1, v))
  }
  return DEFAULT_SFX_VOLUME
}

export const sfxVolume = writable<number>(loadSfxVolume())
export const sfxMuted = writable<boolean>(
  storage?.getItem(STORAGE_KEY_MUTED) === 'true'
)

let volumeSaveTimer: ReturnType<typeof setTimeout> | undefined

sfxVolume.subscribe((v) => {
  if (!storage) return
  clearTimeout(volumeSaveTimer)
  volumeSaveTimer = setTimeout(
    () => storage.setItem(STORAGE_KEY_VOLUME, String(v)),
    300
  )
})

sfxMuted.subscribe((m) => {
  storage?.setItem(STORAGE_KEY_MUTED, String(m))
})

// Multiplier applied on top of each sound's baseline volume so the Settings
// SFX slider/mute scales all effects uniformly.
export function getSfxMultiplier(): number {
  return get(sfxMuted) ? 0 : get(sfxVolume)
}

interface AudioPool {
  audios: HTMLAudioElement[]
  index: number
}

// One pool per url, shared by every group. Playback volume is applied per
// play, so a url two groups happen to share still sounds right in both.
const pools = new Map<string, AudioPool>()

function canUseAudio(): boolean {
  return typeof Audio !== 'undefined'
}

function createAudio(url: string, volume: number): HTMLAudioElement {
  const audio = new Audio(assetUrl(url))
  audio.preload = 'auto'
  audio.volume = volume
  return audio
}

function preloadAudioPool(url: string, volume: number, poolSize: number) {
  if (!canUseAudio() || pools.has(url)) return

  const pool = {
    audios: Array.from({ length: poolSize }, () => createAudio(url, volume)),
    index: 0,
  }

  for (const audio of pool.audios) {
    audio.load()
  }

  pools.set(url, pool)
}

function playAudioFromPool(
  url: string,
  volume: number,
  poolSize: number,
  playbackRate = 1
) {
  preloadAudioPool(url, volume, poolSize)

  const pool = pools.get(url)
  if (!pool) return

  const audio = pool.audios[pool.index]
  pool.index = (pool.index + 1) % pool.audios.length

  const effectiveVolume = volume * getSfxMultiplier()
  if (effectiveVolume <= 0) return

  try {
    audio.currentTime = 0
    audio.volume = effectiveVolume
    audio.playbackRate = playbackRate
    audio.preservesPitch = playbackRate === 1
    audio.play().catch(() => {})
    return audio
  } catch {
    // Browser audio policies can reject playback until the first user gesture.
  }
}

function preloadSounds(table: SoundTable) {
  for (const { url, volume, pool } of Object.values(table)) {
    preloadAudioPool(url, volume, pool)
  }
}

function playSound({ url, volume, pool }: SoundSpec) {
  if (!canUseAudio()) return
  playAudioFromPool(url, volume, pool)
}

export function preloadSwordHitSound() {
  for (const url of getAllMaterialHitSoundUrls()) {
    preloadAudioPool(url, SWORD_HIT_VOLUME, SWORD_HIT_POOL_SIZE)
  }
}

export function preloadSwordMissSound() {
  for (const url of getAllMaterialMissSoundUrls()) {
    preloadAudioPool(url, SWORD_MISS_VOLUME, SWORD_MISS_POOL_SIZE)
  }
}

export function playSwordHitSound(url = DEFAULT_MATERIAL_HIT_SOUND_URL) {
  if (!canUseAudio()) return
  playAudioFromPool(url, SWORD_HIT_VOLUME, SWORD_HIT_POOL_SIZE)
}

export function playSwordMissSound(
  url = DEFAULT_MATERIAL_MISS_SOUND_URL,
  delayMs = 0
) {
  if (!canUseAudio()) return
  if (delayMs > 0) {
    window.setTimeout(() => playSwordMissSound(url), delayMs)
    return
  }
  playAudioFromPool(url, SWORD_MISS_VOLUME, SWORD_MISS_POOL_SIZE)
}

/** Every sound the monster defs name. */
export function preloadMonsterSounds() {
  for (const def of Object.values(monsterDefs)) {
    if (def.deathSound) {
      preloadAudioPool(
        def.deathSound,
        MONSTER_DEATH_VOLUME,
        MONSTER_DEATH_POOL_SIZE
      )
    }
    for (const url of [
      def.attackSound,
      def.rangedAttackSound,
      def.wakeSound,
      def.sleepSound,
      def.crumbleSound,
    ]) {
      if (url) preloadMonsterSound(url)
    }
  }
}

export function preloadMonsterSound(url: string) {
  preloadAudioPool(url, MONSTER_SOUND_VOLUME, MONSTER_SOUND_POOL_SIZE)
}

/** A monster's blow, skill, waking or crumbling; `delayMs` lines it up with
 *  the frame it belongs to. */
export function playMonsterSound(url: string, delayMs = 0) {
  if (!canUseAudio()) return
  const play = () =>
    playAudioFromPool(url, MONSTER_SOUND_VOLUME, MONSTER_SOUND_POOL_SIZE)
  if (delayMs > 0) window.setTimeout(play, delayMs)
  else play()
}

export function playMonsterDeathSound(url: string) {
  if (!canUseAudio()) return
  playAudioFromPool(url, MONSTER_DEATH_VOLUME, MONSTER_DEATH_POOL_SIZE)
}

export function preloadPlayerHurtSounds() {
  preloadSounds(PLAYER_HURT_SOUNDS)
}

// A cry already in the air wins: restarting it mid-breath reads as a stutter
// rather than a second blow.
let hurtVoice: HTMLAudioElement | undefined

/** `delayMs` lines the cry up with the monster's impact frame. */
export function playPlayerHurtSound(gender: Gender, delayMs = 0) {
  const spec = PLAYER_HURT_SOUNDS[gender]
  if (!spec || !canUseAudio()) return
  if (delayMs > 0) {
    window.setTimeout(() => playPlayerHurtSound(gender), delayMs)
    return
  }
  if (hurtVoice && !hurtVoice.paused && !hurtVoice.ended) return
  hurtVoice = playAudioFromPool(spec.url, spec.volume, spec.pool)
}

export function preloadPlayerDeathSounds() {
  preloadSounds(PLAYER_DEATH_SOUNDS)
}

/** Immediate: the collapse starts at the killing-blow message, so the cry
 *  plays with it rather than waiting for the attack's impact frame. */
export function playPlayerDeathSound(gender: Gender) {
  playSound(PLAYER_DEATH_SOUNDS[gender])
}

export function preloadPropSounds() {
  preloadSounds(PROP_SOUNDS)
}

/** `break`: barrel/crate shatter, timed by the caller to the slash contact
 *  frame. `chestOpen`: lid swing on the open broadcast. `coinSpill`: the
 *  pile's pour clip starting. `coinJackpot`: the picker's jackpot coin pile.
 *  `keyDrop`: a dungeon key falling as loot. `pickaxe`: each mining swing,
 *  `delayMs` after it starts so it lands with the point. */
export function playPropSound(kind: PropSound, delayMs = 0) {
  if (delayMs > 0) {
    window.setTimeout(() => playPropSound(kind), delayMs)
    return
  }
  playSound(PROP_SOUNDS[kind])
}

/** Load dungeon effects on entry. */
export function preloadDungeonSounds() {
  preloadSounds(DUNGEON_SOUNDS)
  preloadSounds(CART_SOUNDS)
}

export function playDungeonSound(kind: DungeonSound) {
  playSound(DUNGEON_SOUNDS[kind])
}

const distanceGain = (distance: number, range: number) =>
  Math.max(0, 1 - distance / range) ** 2

function pauseSounds(specs: readonly SoundSpec[]) {
  for (const { url } of specs) {
    for (const audio of pools.get(url)?.audios ?? []) audio.pause()
  }
}

export function playDungeonDripSound(distance: number, seed: number) {
  const gain = distanceGain(distance, 10)
  if (gain <= 0 || getSfxMultiplier() <= 0) return
  const { url, volume, pool } =
    DUNGEON_DRIP_SOUNDS[Math.floor(Math.random() * DUNGEON_DRIP_SOUNDS.length)]
  playAudioFromPool(url, volume * gain, pool, 0.92 + (seed % 1) * 0.16)
}

const cartSoundTimers = new Set<number>()

function laterCartSound(ms: number, fn: () => void) {
  const id = window.setTimeout(() => {
    cartSoundTimers.delete(id)
    fn()
  }, ms)
  cartSoundTimers.add(id)
}

/** A cart rolling `seconds` and stopping dead `distance` metres away. */
export function playCartRollSound(seconds: number, distance: number) {
  const gain = distanceGain(distance, CART_SOUND_RANGE)
  if (gain <= 0 || getSfxMultiplier() <= 0 || !canUseAudio()) return
  const { roll, crash } = CART_SOUNDS
  const rolling = playAudioFromPool(roll.url, roll.volume * gain, roll.pool)
  if (rolling) {
    rolling.loop = true
    laterCartSound(seconds * 1000, () => {
      rolling.loop = false
      rolling.pause()
    })
  }
  laterCartSound(Math.max(0, seconds - CART_CRASH_LEAD_S) * 1000, () =>
    playAudioFromPool(crash.url, crash.volume * gain, crash.pool)
  )
}

export function stopCartSounds() {
  for (const id of cartSoundTimers) window.clearTimeout(id)
  cartSoundTimers.clear()
  pauseSounds(Object.values(CART_SOUNDS))
}

export function stopDungeonDripSounds() {
  pauseSounds(DUNGEON_DRIP_SOUNDS)
}

export function preloadAbilitySounds() {
  preloadSounds(ABILITY_SOUNDS)
}

export function playPrimalRoarSound(gender: Gender, delayMs = 0) {
  const spec = PRIMAL_ROAR_SOUNDS[gender]
  if (delayMs > 0) {
    preloadAudioPool(spec.url, spec.volume, spec.pool)
    window.setTimeout(() => playSound(spec), delayMs)
    return
  }
  playSound(spec)
}

// One march per tune the server picks (`BATTLE_HYMN_TUNES`).
export const BATTLE_HYMN_TUNES = [
  '/sounds/battle-hymn-1.ogg',
  '/sounds/battle-hymn-2.ogg',
  '/sounds/battle-hymn-3.ogg',
]
const BATTLE_HYMN_VOLUME = 0.45
const battleHymns = new Map<number, HTMLAudioElement>()

/** A bard's march at `gain` (distance falloff), cut short by
 *  `stopBattleHymn`. False when nothing is heard. */
export function playBattleHymn(bardId: number, tune: number, gain: number) {
  stopBattleHymn(bardId)
  const url = BATTLE_HYMN_TUNES[tune]
  const volume = Math.min(1, BATTLE_HYMN_VOLUME * gain * getSfxMultiplier())
  if (!canUseAudio() || !url || volume <= 0) return false
  const audio = createAudio(url, volume)
  const forget = () => {
    if (battleHymns.get(bardId) === audio) battleHymns.delete(bardId)
  }
  audio.onended = forget
  battleHymns.set(bardId, audio)
  audio.play().catch(forget)
  return true
}

/** True when a march was playing. */
export function stopBattleHymn(bardId: number) {
  const audio = battleHymns.get(bardId)
  if (!audio) return false
  battleHymns.delete(bardId)
  audio.pause()
  return true
}

export function playAbilitySound(ability: AbilityId) {
  const sound = ABILITY_SOUNDS[ability]
  if (sound) playSound(sound)
}

export function preloadLevelUpSound() {
  preloadSounds({ levelUp: LEVEL_UP_SOUND })
}

export function playLevelUpSound(gain = 1) {
  if (!canUseAudio()) return
  const { url, volume, pool } = LEVEL_UP_SOUND
  playAudioFromPool(url, volume * gain, pool)
}

export function preloadBowSounds() {
  preloadSounds(BOW_SOUNDS)
}

/** `delayMs` lines the loose up with the release frame of the shot clip, the
 *  same moment the melee whoosh uses. */
export function playBowSound(kind: BowSound, delayMs = 0) {
  if (!canUseAudio()) return
  if (delayMs > 0) {
    window.setTimeout(() => playBowSound(kind), delayMs)
    return
  }
  playSound(BOW_SOUNDS[kind])
}

export function preloadFishingSounds() {
  preloadSounds(FISHING_SOUNDS)
}

const pendingFishingTimers = new Set<number>()

/** `delayMs` lets e.g. the splash line up with the bobber landing rather
 *  than the swing that threw it (same pattern as `playSwordMissSound`). */
export function playFishingSound(kind: FishingSound, delayMs = 0) {
  if (!canUseAudio()) return
  if (delayMs > 0) {
    const timer = window.setTimeout(() => {
      pendingFishingTimers.delete(timer)
      playFishingSound(kind)
    }, delayMs)
    pendingFishingTimers.add(timer)
    return
  }
  playSound(FISHING_SOUNDS[kind])
}

/** A cast aborted mid-flight must not splash after the line is back in. */
export function cancelPendingFishingSounds() {
  for (const timer of pendingFishingTimers) window.clearTimeout(timer)
  pendingFishingTimers.clear()
}
