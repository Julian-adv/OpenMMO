import { derived, get } from 'svelte/store'
import titlesJson from '../../../../data/titles.json'
import { locale, translate, type Locale, type MessageKey } from '../i18n'

interface TitleDef {
  id: string
  name: string
  nameKo: string
  order: number
}

const defs = titlesJson as Record<string, TitleDef>

function nameIn(id: string, language: Locale): string {
  const def = defs[id]
  if (!def) return id
  return translate(
    `title.${id}` as MessageKey,
    { defaultValue: language === 'ko' ? def.nameKo || def.name : def.name },
    language
  )
}

/** Follow the game language for displayed titles. */
export const titleName = derived(
  locale,
  (language) => (id: string) => nameIn(id, language)
)

/** Non-reactive lookup for one-off text (chat lines). */
export function titleNameNow(id: string): string {
  return get(titleName)(id)
}
