import { get, writable } from 'svelte/store'

const BGM_FILES = [
  'Untitled.mp3',
  'Untitled (1).mp3',
  'Untitled (2).mp3',
  'Untitled (3).mp3',
  'Untitled (4).mp3',
  'Untitled (5).mp3',
  'Untitled (6).mp3',
  'Untitled (7).mp3',
  'Untitled (8).mp3',
  'Untitled (9).mp3',
]

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

function applyVolume() {
  if (audio) audio.volume = get(bgmMuted) ? 0 : get(bgmVolume)
}

bgmVolume.subscribe((v) => {
  clearTimeout(volumeSaveTimer)
  volumeSaveTimer = setTimeout(
    () => localStorage.setItem(STORAGE_KEY_VOLUME, String(v)),
    300
  )
  applyVolume()
})

bgmMuted.subscribe((m) => {
  localStorage.setItem(STORAGE_KEY_MUTED, String(m))
  applyVolume()
})

function shufflePlaylist() {
  playlist = [...BGM_FILES]
  for (let i = playlist.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1))
    ;[playlist[i], playlist[j]] = [playlist[j], playlist[i]]
  }
  playlistIndex = 0
}

function playNext() {
  if (playlistIndex >= playlist.length) {
    shufflePlaylist()
  }

  const file = playlist[playlistIndex++]
  currentBgmTrack.set(file.replace('.mp3', ''))

  if (!audio) {
    audio = new Audio()
    audio.addEventListener('ended', playNext)
    audio.addEventListener('error', playNext)
  }

  applyVolume()
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
