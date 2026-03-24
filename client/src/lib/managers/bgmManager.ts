import { get, writable } from 'svelte/store'

const BGM_FILES = [
  'Lonely Steppe of Ages.mp3',
  'Lonely Steppe of Ages (1).mp3',
  'Winds of the Open Plain.mp3',
  'Dawn Over the Kingdom.mp3',
  'Beyond the Horizon.mp3',
  'Quiet Longing.mp3',
  'Twilight Fields.mp3',
  'Hearthside Respite.mp3',
  'Hearthside Respite (1).mp3',
  'Triumphal Procession.mp3',
  'Triumphal Procession (1).mp3',
  'Wanderer of the Old Fields (1).mp3',
  'Lonely Roads of Eldoria.mp3',
  'Crescendo of Remembering.mp3',
  'Sky of Burning Wings.mp3',
  'Sky of Burning Wings (1).mp3',
]

const BATTLE_BGM_FILES = [
  'Blood and Bronze.mp3',
  'Blood and Bronze (1).mp3',
  'Drums of Valor.mp3',
]
const BATTLE_LINGER_MS = 5000
const BATTLE_FADE_OUT_MS = 3000
const BATTLE_FADE_STEP_MS = 50
const BATTLE_QUIET_MIN_SEC = 5
const BATTLE_QUIET_MAX_SEC = 20

const MIN_QUIET_SEC = 0
const MAX_QUIET_SEC = 60

const STORAGE_KEY_VOLUME = 'onlinerpg_bgmVolume'
const STORAGE_KEY_MUTED = 'onlinerpg_bgmMuted'
const DEFAULT_VOLUME = 0.1

function loadVolume(): number {
  const saved = localStorage.getItem(STORAGE_KEY_VOLUME)
  if (saved !== null) {
    const v = parseFloat(saved)
    if (!isNaN(v)) return Math.max(0, Math.min(1, v))
  }
  return DEFAULT_VOLUME
}

export const currentBgmTrack = writable<string>('')
export const bgmVolume = writable<number>(loadVolume())
export const bgmMuted = writable<boolean>(
  localStorage.getItem(STORAGE_KEY_MUTED) === 'true'
)

let audio: HTMLAudioElement | null = null
let playlist: string[] = []
let playlistIndex = 0
let volumeSaveTimer: ReturnType<typeof setTimeout> | undefined

function getTargetVolume(): number {
  return get(bgmMuted) ? 0 : get(bgmVolume)
}

function applyVolume(el: HTMLAudioElement | null) {
  if (el) el.volume = getTargetVolume()
}

let isFadingOut = false
let battleAudio: HTMLAudioElement | null = null

bgmVolume.subscribe((v) => {
  clearTimeout(volumeSaveTimer)
  volumeSaveTimer = setTimeout(
    () => localStorage.setItem(STORAGE_KEY_VOLUME, String(v)),
    300
  )
  applyVolume(audio)
  if (!isFadingOut) applyVolume(battleAudio)
})

bgmMuted.subscribe((m) => {
  localStorage.setItem(STORAGE_KEY_MUTED, String(m))
  applyVolume(audio)
  if (!isFadingOut) applyVolume(battleAudio)
})

function shufflePlaylist() {
  playlist = [...BGM_FILES]
  for (let i = playlist.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1))
    ;[playlist[i], playlist[j]] = [playlist[j], playlist[i]]
  }
  playlistIndex = 0
}

let quietTimer: ReturnType<typeof setTimeout> | undefined
let isFirstTrack = true

function playNext() {
  if (isBattlePlaying) return
  if (!isFirstTrack) {
    const delaySec =
      MIN_QUIET_SEC + Math.random() * (MAX_QUIET_SEC - MIN_QUIET_SEC)
    currentBgmTrack.set('')
    clearTimeout(quietTimer)
    quietTimer = setTimeout(playTrack, delaySec * 1000)
    return
  }
  isFirstTrack = false
  playTrack()
}

function playTrack() {
  if (isBattlePlaying) return
  if (playlistIndex >= playlist.length) {
    shufflePlaylist()
  }

  const file = playlist[playlistIndex++]
  const trackName = file.replace('.mp3', '')

  if (!audio) {
    audio = new Audio()
    audio.addEventListener('ended', playNext)
    audio.addEventListener('error', playNext)
    audio.addEventListener('playing', () => {
      currentBgmTrack.set(audio!.dataset.trackName ?? '')
    })
  }

  applyVolume(audio)
  audio.dataset.trackName = trackName
  audio.src = `/bgm/${file}`
  audio.play().catch(() => {})
}

let started = false

export function startBgm() {
  if (started) return
  started = true
  shufflePlaylist()
  playNext()
}

// --- Battle music ---

let battleLingerTimer: ReturnType<typeof setTimeout> | undefined
let battleFadeTimer: ReturnType<typeof setInterval> | undefined
let battleQuietTimer: ReturnType<typeof setTimeout> | undefined
let isBattlePlaying = false

export function startBattleMusic() {
  if (isBattlePlaying) return
  isBattlePlaying = true
  isFadingOut = false

  // Pause normal BGM
  clearTimeout(quietTimer)
  if (audio) {
    audio.pause()
  }
  currentBgmTrack.set('')

  // Clear any pending linger/fade/quiet from a previous battle
  clearTimeout(battleLingerTimer)
  clearInterval(battleFadeTimer)
  clearTimeout(battleQuietTimer)

  const file =
    BATTLE_BGM_FILES[Math.floor(Math.random() * BATTLE_BGM_FILES.length)]
  const trackName = file.replace('.mp3', '')

  if (!battleAudio) {
    battleAudio = new Audio()
    battleAudio.loop = true
    battleAudio.addEventListener('playing', () => {
      currentBgmTrack.set(battleAudio!.dataset.trackName ?? '')
    })
  }

  applyVolume(battleAudio)
  battleAudio.dataset.trackName = trackName
  battleAudio.currentTime = 0
  battleAudio.src = `/bgm/${file}`
  battleAudio.play().catch(() => {})
}

export function stopBattleMusic() {
  if (!isBattlePlaying) return
  isBattlePlaying = false

  if (!battleAudio) {
    resumeNormalBgm()
    return
  }

  // Wait a bit before fading out
  clearTimeout(battleLingerTimer)
  battleLingerTimer = setTimeout(fadeOutBattleMusic, BATTLE_LINGER_MS)
}

function fadeOutBattleMusic() {
  if (isBattlePlaying || !battleAudio) return

  const startVol = battleAudio.volume
  if (startVol === 0) {
    battleAudio.pause()
    currentBgmTrack.set('')
    scheduleNormalBgmResume()
    return
  }

  isFadingOut = true
  const steps = BATTLE_FADE_OUT_MS / BATTLE_FADE_STEP_MS
  const volStep = startVol / steps
  let remaining = steps

  clearInterval(battleFadeTimer)
  battleFadeTimer = setInterval(() => {
    remaining--
    if (remaining <= 0 || !battleAudio) {
      clearInterval(battleFadeTimer)
      isFadingOut = false
      if (battleAudio) {
        battleAudio.pause()
        battleAudio.volume = startVol
      }
      currentBgmTrack.set('')
      scheduleNormalBgmResume()
      return
    }
    battleAudio!.volume = Math.max(0, battleAudio!.volume - volStep)
  }, BATTLE_FADE_STEP_MS)
}

function scheduleNormalBgmResume() {
  const delaySec =
    BATTLE_QUIET_MIN_SEC +
    Math.random() * (BATTLE_QUIET_MAX_SEC - BATTLE_QUIET_MIN_SEC)
  clearTimeout(battleQuietTimer)
  battleQuietTimer = setTimeout(resumeNormalBgm, delaySec * 1000)
}

function resumeNormalBgm() {
  if (isBattlePlaying) return
  if (!audio) {
    playTrack()
    return
  }
  if (audio.ended || !audio.src) {
    playTrack()
  } else {
    applyVolume(audio)
    audio.play().catch(() => {})
    currentBgmTrack.set(audio.dataset.trackName ?? '')
  }
}
