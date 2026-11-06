import { readFileSync } from 'node:fs'
import { writeFile, rename } from 'node:fs/promises'

// The slider list a preview panel shows, so the route checks the same ranges.
export const sliderList = name =>
  JSON.parse(readFileSync(new URL(`../../client/src/previews/${name}`, import.meta.url), 'utf8'))

/** A dev-server route taking a preview slider panel's POSTed values: checks
 *  each slider's range, then `save(input)` may return `{ status, error }`. */
export function sliderSaveRoute(route, sliders, save) {
  return (req, res, next) => {
    if (req.url?.split('?')[0] !== route) return next()
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
      res.end(JSON.stringify(value))
    }
    void (async () => {
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
      for (const { key, min, max } of sliders) {
        const value = input?.[key]
        if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max)
          return reply(400, { error: `값 범위를 확인하세요: ${key}` })
      }
      const failed = await save(input)
      if (failed) return reply(failed.status, { error: failed.error })
      reply(200, { saved: true })
    })().catch(next)
  }
}

/** Replace `target` in one step, so a watcher never reads half a file. */
export async function writeAtomic(target, text) {
  const temporary = new URL(target.href + '.tmp')
  await writeFile(temporary, text)
  await rename(temporary, target)
}
