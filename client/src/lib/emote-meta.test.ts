import { afterEach, describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import {
  EMOTE_INTENT_TTL_MS,
  EMOTE_LIST,
  emoteClickCommand,
  emoteLabel,
} from './emote-meta'
import {
  EXCAVATE_EMOTE_ANIM,
  HELD_EMOTE_ANIMS,
  isEmoteAnim,
  isSelfEndingEmote,
  JOIN_DANCE_MAX_RISE_M,
  JOIN_DANCE_RANGE_M,
  JOINABLE_DANCE_ANIMS,
  LOOPING_EMOTE_ANIMS,
  MUSIC_EMOTE_ANIM,
  ONE_SHOT_EMOTE_ANIMS,
} from './stores/emoteStore'
import { languagePreference } from './i18n'

afterEach(() => languagePreference.set('en'))

// Detect drift from the server's supported animations.
function rustEmoteList(constName: string): string[] {
  const source = readFileSync(
    path.resolve(__dirname, '../../../shared/src/messages.rs'),
    'utf-8'
  )
  // Anchored on the declaration so a doc-comment example can't hijack it.
  const match = source.match(
    new RegExp(`pub const ${constName}\\s*:[^=]*=\\s*&\\[([^\\]]*)\\]`)
  )
  if (!match) throw new Error(`${constName} not found in messages.rs`)
  return [...match[1].matchAll(/"([^"]+)"/g)].map((m) => m[1])
}

describe('emote panel metadata', () => {
  it('recognizes NPC excavation as a held interaction without exposing it in the player menu', () => {
    const rust = readFileSync(
      path.resolve(__dirname, '../../../shared/src/messages.rs'),
      'utf-8'
    )
    expect(rust).toContain(
      `pub const EXCAVATE_EMOTE: &str = "${EXCAVATE_EMOTE_ANIM}";`
    )
    expect(isEmoteAnim(EXCAVATE_EMOTE_ANIM)).toBe(true)
    expect(HELD_EMOTE_ANIMS.has(EXCAVATE_EMOTE_ANIM)).toBe(true)
    expect(isSelfEndingEmote(EXCAVATE_EMOTE_ANIM)).toBe(false)
    expect(EMOTE_LIST.some((emote) => emote.anim === EXCAVATE_EMOTE_ANIM)).toBe(
      false
    )
  })
  it('mirrors ONE_SHOT_EMOTES from shared/src/messages.rs', () => {
    expect(new Set(rustEmoteList('ONE_SHOT_EMOTES'))).toEqual(
      ONE_SHOT_EMOTE_ANIMS
    )
  })

  it('mirrors LOOPING_EMOTES from shared/src/messages.rs', () => {
    expect(new Set(rustEmoteList('LOOPING_EMOTES'))).toEqual(
      LOOPING_EMOTE_ANIMS
    )
  })

  it('mirrors JOINABLE_DANCES and its range from shared/src/messages.rs', () => {
    expect(new Set(rustEmoteList('JOINABLE_DANCES'))).toEqual(
      JOINABLE_DANCE_ANIMS
    )
    for (const dance of JOINABLE_DANCE_ANIMS) {
      expect(LOOPING_EMOTE_ANIMS.has(dance), dance).toBe(true)
    }
    const rust = readFileSync(
      path.resolve(__dirname, '../../../shared/src/messages.rs'),
      'utf-8'
    )
    const range = rust.match(/pub const JOIN_DANCE_RANGE_M: f32 = ([\d.]+);/)
    expect(Number(range?.[1])).toBe(JOIN_DANCE_RANGE_M)
    const rise = rust.match(/pub const JOIN_DANCE_MAX_RISE_M: f32 = ([\d.]+);/)
    expect(Number(rise?.[1])).toBe(JOIN_DANCE_MAX_RISE_M)
  })

  it('derives one labelled entry per accepted emote', () => {
    expect(new Set(EMOTE_LIST.map((e) => e.anim))).toEqual(
      new Set([...ONE_SHOT_EMOTE_ANIMS, ...LOOPING_EMOTE_ANIMS])
    )
    for (const emote of EMOTE_LIST) {
      expect(emote.label.length).toBeGreaterThan(0)
      expect(emote.loops).toBe(LOOPING_EMOTE_ANIMS.has(emote.anim))
    }
  })
})

describe('localized emote labels', () => {
  const instrument = {
    anim: MUSIC_EMOTE_ANIM,
    label: 'Play Instrument',
    loops: true,
  }

  it.each(['ko', 'ja', 'zh-Hans', 'zh-Hant'] as const)(
    '%s translates every supported emote and instrument preview',
    (language) => {
      for (const emote of [...EMOTE_LIST, instrument]) {
        expect(emoteLabel(emote, 'en')).toBe(emote.label)
        expect(emoteLabel(emote, language), emote.anim).not.toBe(emote.label)
        expect(emoteLabel(emote, language).trim(), emote.anim).not.toBe('')
      }
    }
  )

  it('updates a retained preview when the language changes', () => {
    const previewed = EMOTE_LIST.find((emote) => emote.anim === 'clap')!
    languagePreference.set('ko')
    expect(emoteLabel(previewed)).toBe('박수')
    expect(emoteLabel(instrument)).toBe('악기 연주')
    languagePreference.set('ja')
    expect(emoteLabel(previewed)).toBe('拍手')
    languagePreference.set('zh-Hans')
    expect(emoteLabel(previewed)).toBe('鼓掌')
    expect(previewed).toEqual({ anim: 'clap', label: 'Clap', loops: false })
  })

  it('keeps future emotes readable before translations are added', () => {
    expect(
      emoteLabel(
        { anim: 'future_wave', label: 'Future Wave', loops: false },
        'ko'
      )
    ).toBe('Future Wave')
  })
})

describe('emoteClickCommand', () => {
  const dance = { anim: 'twist', label: 'Twist', loops: true }
  const other = { anim: 'macarena', label: 'Macarena', loops: true }
  const oneShot = { anim: 'clap', label: 'Clap', loops: false }

  it('toggles a settled looping emote to stop', () => {
    expect(emoteClickCommand(dance, 'twist', null, 1000)).toBe('stop')
    expect(emoteClickCommand(dance, null, null, 1000)).toBe('play')
  })

  it('plays, not stops, when a different emote was just commanded', () => {
    // Server echo still says twist, but the player already clicked macarena.
    const intent = { anim: other.anim, at: 900 }
    expect(emoteClickCommand(dance, 'twist', intent, 1000)).toBe('play')
  })

  it('replays an emote the player just stopped despite the stale echo', () => {
    const intent = { anim: null, at: 900 }
    expect(emoteClickCommand(dance, 'twist', intent, 1000)).toBe('play')
  })

  it('stops a re-click of the emote just commanded', () => {
    const intent = { anim: dance.anim, at: 900 }
    expect(emoteClickCommand(dance, null, intent, 1000)).toBe('stop')
  })

  it('falls back to the server echo once the intent expires', () => {
    const intent = { anim: other.anim, at: 0 }
    expect(
      emoteClickCommand(dance, 'twist', intent, EMOTE_INTENT_TTL_MS + 1)
    ).toBe('stop')
  })

  it('never stops a one-shot emote', () => {
    expect(emoteClickCommand(oneShot, 'clap', null, 1000)).toBe('play')
  })
})
