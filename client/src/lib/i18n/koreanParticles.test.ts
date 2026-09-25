import { describe, expect, it } from 'vitest'
import { resolveKoreanParticles } from './koreanParticles'

describe('Korean particle tokens', () => {
  it.each([
    ['검$을', '검을'],
    ['도끼$을', '도끼를'],
    ['방패$은', '방패는'],
    ['검$은', '검은'],
    ['활$이', '활이'],
    ['마나$이', '마나가'],
    ['검$과', '검과'],
    ['도끼$과', '도끼와'],
    ['검$으로', '검으로'],
    ['도끼$으로', '도끼로'],
    ['활$으로', '활로'],
    ['가$을 각$을 히$을 힣$을', '가를 각을 히를 힣을'],
    ['"검"$을', '"검"을'],
    ['(도끼)$을', '(도끼)를'],
    ['검$에게', '검$에게'],
    ['$을', '를'],
    ['MP$을', 'MP를'],
    ['🪓$을', '🪓를'],
  ])('%s → %s', (text, expected) => {
    expect(resolveKoreanParticles(text)).toBe(expected)
  })

  it('recognizes decomposed syllables without modifying the original word', () => {
    for (const [word, particle] of [
      ['검', '을'],
      ['도끼', '를'],
      ['활', '을'],
    ]) {
      const decomposed = word.normalize('NFD')
      expect(resolveKoreanParticles(`${decomposed}$을`)).toBe(
        decomposed + particle
      )
    }
  })

  it.each([
    ['0', '을', '으로'],
    ['1', '을', '로'],
    ['2', '를', '로'],
    ['3', '을', '으로'],
    ['4', '를', '로'],
    ['5', '를', '로'],
    ['6', '을', '으로'],
    ['7', '을', '로'],
    ['8', '을', '로'],
    ['9', '를', '로'],
    ['1,000', '을', '으로'],
  ])('handles numeric endings: %s', (number, object, direction) => {
    expect(resolveKoreanParticles(`${number}$을`)).toBe(number + object)
    expect(resolveKoreanParticles(`${number}$으로`)).toBe(number + direction)
  })
})
