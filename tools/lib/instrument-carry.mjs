import { readFile, writeFile, rename } from 'node:fs/promises'

// How a carried instrument sits in the hand; the game reads this file through handProps.ts.
const target = new URL('../../client/src/lib/data/instrumentCarry.json', import.meta.url)
const route = '/__female-preview/instrument-carry.json'
// Keep in sync with the sliders in client/src/previews/female-instrument-carry.ts.
const ranges = {
  grip: [0, 0.75], tilt: [-30, 90], splay: [-45, 45], twist: [-180, 180], front: [-0.1, 0.1], palm: [-0.1, 0.1],
}

export function instrumentCarry() {
  return (req, res, next) => {
    if (req.url?.split('?')[0] !== route) return next()
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
      res.end(JSON.stringify(value))
    }
    void (async () => {
      if (req.method === 'GET') return reply(200, JSON.parse(await readFile(target, 'utf8')))
      if (req.method !== 'POST') return reply(405, { error: '지원하지 않는 요청입니다.' })
      if (req.headers['sec-fetch-site'] === 'cross-site' ||
          !req.headers['content-type']?.startsWith('application/json'))
        return reply(400, { error: '올바른 저장 요청이 아닙니다.' })
      let raw = ''
      for await (const chunk of req) {
        raw += chunk
        if (raw.length > 4096) return reply(413, { error: '저장값이 너무 큽니다.' })
      }
      let input
      try { input = JSON.parse(raw) } catch { return reply(400, { error: '저장값을 읽을 수 없습니다.' }) }
      const carry = {}
      for (const [key, [min, max]] of Object.entries(ranges)) {
        const value = input?.[key]
        if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max)
          return reply(400, { error: `값 범위를 확인하세요: ${key}` })
        carry[key] = Math.round(value * 10000) / 10000
      }
      await writeFile(new URL(target.href + '.tmp'), JSON.stringify(carry, null, 2) + '\n')
      await rename(new URL(target.href + '.tmp'), target)
      reply(200, { saved: true })
    })().catch(next)
  }
}
