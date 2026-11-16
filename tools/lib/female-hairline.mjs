import { readFile } from 'node:fs/promises'
import { writeAtomic } from './slider-save-route.mjs'

// Centimetres each listed hair's front hairline slides down the forehead; baked by
// tools/blender-scripts/fit_female_long_hair.py.
const target = new URL('../../doc/assets/female-hairline.json', import.meta.url)

export function femaleHairline(file = target) {
  let writing = Promise.resolve()
  return (req, res, next) => {
    if (req.url?.split('?')[0] !== '/__female-preview/hairline.json') return next()
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
        if (raw.length > 256) return reply(413, { error: '저장값이 너무 큽니다.' })
      }
      let input
      try { input = JSON.parse(raw) } catch { return reply(400, { error: '저장값을 읽을 수 없습니다.' }) }
      const { hair, drop_cm: drop } = input ?? {}
      if (typeof drop !== 'number' || !(drop >= 0 && drop <= 6))
        return reply(400, { error: '값(0–6cm)을 확인하세요.' })
      const save = writing.then(async () => {
        const saved = JSON.parse(await readFile(file, 'utf8'))
        if (!Object.hasOwn(saved, hair)) return false
        saved[hair].pending_cm = Math.round(drop * 10) / 10
        await writeAtomic(file, JSON.stringify(saved, null, 2) + '\n')
        return true
      })
      writing = save.catch(() => {})
      if (await save) reply(200, { saved: true })
      else reply(400, { error: '헤어를 확인하세요.' })
    })().catch(next)
  }
}
