import { translate, type Locale, type MessageKey } from './i18n'
import {
  LOOPING_EMOTE_ANIMS,
  MUSIC_EMOTE_ANIM,
  ONE_SHOT_EMOTE_ANIMS,
} from './stores/emoteStore'

/** Panel entries follow the server's supported animations. */
export interface EmoteMeta {
  anim: string
  label: string
  /** Loops until the player moves or stops it; one-shots end on their own. */
  loops: boolean
}

/** Anims whose mechanical label reads badly (`stand_pose2` → "Stand Pose2"). */
const LABEL_OVERRIDES: Record<string, string> = {
  stand_pose2: 'Pose 2',
  stand_pose3: 'Pose 3',
  stand_pose4: 'Pose 4',
}

const LABEL_KEYS: Partial<Record<string, MessageKey>> = {
  excited: 'emote.excited',
  clap: 'emote.clap',
  yawn: 'emote.yawn',
  twist: 'emote.twist',
  macarena: 'emote.macarena',
  chicken: 'emote.chicken',
  stand_pose2: 'emote.stand_pose2',
  stand_pose3: 'emote.stand_pose3',
  stand_pose4: 'emote.stand_pose4',
  weight_shift: 'emote.weight_shift',
  [MUSIC_EMOTE_ANIM]: 'emotes.playInstrument',
}

export function emoteLabel(emote: EmoteMeta, language?: Locale): string {
  const key = LABEL_KEYS[emote.anim]
  return key ? translate(key, {}, language) : emote.label
}

function labelFor(anim: string): string {
  return (
    LABEL_OVERRIDES[anim] ??
    anim
      .split('_')
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join(' ')
  )
}

export const EMOTE_LIST: EmoteMeta[] = [
  ...ONE_SHOT_EMOTE_ANIMS,
  ...LOOPING_EMOTE_ANIMS,
].map((anim) => ({
  anim,
  label: labelFor(anim),
  loops: LOOPING_EMOTE_ANIMS.has(anim),
}))

/** The player's last click — anim they commanded (null = stop) and when. */
export interface EmoteIntent {
  anim: string | null
  at: number
}

/** After this long an unconfirmed intent is assumed rejected by the server. */
export const EMOTE_INTENT_TTL_MS = 2000

/** Use pending intent to handle clicks before the server echo arrives. */
export function emoteClickCommand(
  emote: EmoteMeta,
  active: string | null,
  intent: EmoteIntent | null,
  now: number
): 'stop' | 'play' {
  const current =
    intent && now - intent.at <= EMOTE_INTENT_TTL_MS ? intent.anim : active
  return emote.loops && current === emote.anim ? 'stop' : 'play'
}
