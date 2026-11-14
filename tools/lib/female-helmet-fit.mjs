import { readFile, writeFile, rename } from 'node:fs/promises'

const target = new URL('../../client/src/lib/data/femaleHelmetFits.json', import.meta.url)
// Keep in sync with PART_FIT_RANGES in client/src/previews/female-part-fit.ts.
const ranges = {
  'move-x': [-10, 10], 'move-y': [-10, 10], 'move-z': [-10, 10],
  scale: [70, 130], 'scale-x': [70, 130], 'scale-y': [70, 130], 'scale-z': [70, 130],
  'rotate-x': [-45, 45], 'rotate-y': [-45, 45], 'rotate-z': [-45, 45],
}
const defaults = { scale: 100, 'scale-x': 100, 'scale-y': 100, 'scale-z': 100 }

export function femaleHelmetFit(file = target) {
  let writing = Promise.resolve()
  return (req, res, next) => {
    if (req.url?.split('?')[0] !== '/__female-preview/helmet-fit.json') return next()
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
      res.end(JSON.stringify(value))
    }
    void (async () => {
      if (req.method === 'GET') return reply(200, JSON.parse(await readFile(file, 'utf8')))
      if (req.method !== 'POST') return reply(405, { error: '지원하지 않는 요청입니다.' })
      if (req.headers['sec-fetch-site'] === 'cross-site' || !req.headers['content-type']?.startsWith('application/json'))
        return reply(400, { error: '올바른 저장 요청이 아닙니다.' })
      let raw = ''
      for await (const chunk of req) {
        raw += chunk
        if (raw.length > 4096) return reply(413, { error: '저장값이 너무 큽니다.' })
      }
      let input
      try { input = JSON.parse(raw) } catch { return reply(400, { error: '저장값을 읽을 수 없습니다.' }) }
      if (typeof input?.helmet !== 'string' || !/^([a-z]+_)?helmet$/.test(input.helmet) ||
          !['bob', 'long', 'ponytail', 'long_bangs'].includes(input.hair))
        return reply(400, { error: '투구와 헤어를 확인하세요.' })
      const fit = {}
      for (const [key, [min, max]] of Object.entries(ranges)) {
        const value = input.fit?.[key]
        if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max)
          return reply(400, { error: `조절값 범위를 확인하세요: ${key}` })
        fit[key] = Math.round(value * 10000) / 10000
      }
      const unchanged = Object.entries(fit).every(([key, value]) => value === (defaults[key] ?? 0))
      const save = writing.then(async () => {
        const fits = JSON.parse(await readFile(file, 'utf8'))
        if (unchanged) delete fits[input.helmet]?.[input.hair]
        else (fits[input.helmet] ??= {})[input.hair] = fit
        if (fits[input.helmet] && !Object.keys(fits[input.helmet]).length) delete fits[input.helmet]
        const temporary = new URL(file.href + '.tmp')
        await writeFile(temporary, JSON.stringify(fits, null, 2) + '\n')
        await rename(temporary, file)
      })
      writing = save.catch(() => {})
      await save
      reply(200, { saved: true })
    })().catch(next)
  }
}
