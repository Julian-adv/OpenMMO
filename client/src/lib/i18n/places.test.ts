import { afterEach, describe, expect, it } from 'vitest'
import { MAP_LABELS } from '../data/mapLabels'
import { DUNGEON_ENTRANCES } from '../data/dungeonDefs'
import { languagePreference } from './locale'
import { placeName } from './places'
import ko from './locales/ko.places.json'
import ja from './locales/ja.places.json'
import zhHans from './locales/zh-Hans.places.json'

afterEach(() => languagePreference.set('en'))

describe('localized map places', () => {
  const places = [...MAP_LABELS, ...DUNGEON_ENTRANCES]

  it.each([
    ['ko', ko],
    ['ja', ja],
    ['zh-Hans', zhHans],
  ] as const)(
    '%s covers every place and discoverable dungeon',
    (language, names) => {
      expect(Object.keys(names).sort()).toEqual(
        places.map(({ id }) => id).sort()
      )
      for (const { id, name } of places) {
        const localized = placeName(id, name, language)
        expect(localized.trim(), id).not.toBe('')
        expect(localized, id).not.toBe(name)
      }
    }
  )

  it('uses phonetic names and follows language changes without changing source data', () => {
    const island = MAP_LABELS.find(({ id }) => id === 'ashisle')!
    languagePreference.set('ko')
    expect(placeName(island.id, island.name)).toBe('애시아일')
    expect(placeName('silverbight', 'Silverbight')).toBe('실버바이트')
    expect(placeName('old_crypt', 'Old Crypt')).toBe('올드 크립트')
    languagePreference.set('ja')
    expect(placeName(island.id, island.name)).toBe('アッシュアイル')
    languagePreference.set('zh-Hans')
    expect(placeName(island.id, island.name)).toBe('阿什艾尔')
    languagePreference.set('en')
    expect(placeName(island.id, island.name)).toBe('Ashisle')
    expect(island.name).toBe('Ashisle')
  })

  it('retains the original name for places added before their translations', () => {
    for (const language of ['en', 'ko', 'ja', 'zh-Hans'] as const) {
      expect(placeName('future_place', 'New Haven', language)).toBe('New Haven')
    }
  })
})
