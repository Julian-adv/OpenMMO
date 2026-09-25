const particles = {
  을: ['을', '를'],
  은: ['은', '는'],
  이: ['이', '가'],
  과: ['과', '와'],
  으로: ['으로', '로'],
} as const

type Particle = keyof typeof particles
const digitFinals = [21, 8, 0, 16, 0, 0, 1, 8, 8, 0]

function finalConsonant(text: string): number {
  const word = text.normalize('NFC').replace(/[\s\p{P}]+$/gu, '')
  const last = word.at(-1) ?? ''
  const code = last.charCodeAt(0)
  if (code >= 0xac00 && code <= 0xd7a3) return (code - 0xac00) % 28
  if (code >= 0x30 && code <= 0x39) return digitFinals[code - 0x30]
  return 0
}

export function resolveKoreanParticles(text: string): string {
  return text.replace(
    /\$(으로|을|은|이|과)/g,
    (_, particle: Particle, offset: number) => {
      const final = finalConsonant(text.slice(0, offset))
      const hasFinal = final !== 0 && !(particle === '으로' && final === 8)
      return particles[particle][hasFinal ? 0 : 1]
    }
  )
}
