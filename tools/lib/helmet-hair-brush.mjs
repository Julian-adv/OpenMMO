import { readFile, writeFile, rename } from 'node:fs/promises'

// Hair triangles hidden under each female helmet, brushed in the female preview; prepare-female-knight.mjs bakes them in.
const helmetHairBrushPath = new URL('../../doc/assets/female-helmet-hair-brush.json', import.meta.url)
const brushableHelmets = ['helmet', 'barbarian_helmet', 'valkyrie_helmet', 'priest_helmet', 'bard_helmet', 'brilliance_helmet']
const brushableHair = ['hair', 'valkyrie_hair', 'ranger_hair']
const route = '/__female-preview/helmet-hair.json'

export async function loadHelmetHairBrush() {
  try {
    return JSON.parse(await readFile(helmetHairBrushPath, 'utf8'))
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
    return { version: 1, helmets: {} }
  }
}

export function helmetHairBrush() {
  return (req, res, next) => {
    if (req.url?.split('?')[0] !== route) return next()
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
      res.end(JSON.stringify(value))
    }
    void (async () => {
      if (req.method === 'GET') return reply(200, await loadHelmetHairBrush())
      if (req.method !== 'POST') return reply(405, { error: '지원하지 않는 요청입니다.' })
      if (req.headers['sec-fetch-site'] === 'cross-site' ||
          !req.headers['content-type']?.startsWith('application/json'))
        return reply(400, { error: '올바른 저장 요청이 아닙니다.' })
      let raw = ''
      for await (const chunk of req) {
        raw += chunk
        if (raw.length > 512 * 1024) return reply(413, { error: '저장값이 너무 큽니다.' })
      }
      let input
      try { input = JSON.parse(raw) } catch { return reply(400, { error: '저장값을 읽을 수 없습니다.' }) }
      const hair = input?.hair
      if (!brushableHelmets.includes(input?.helmet) || typeof hair !== 'object' || !hair ||
          !Object.entries(hair).every(([name, list]) => brushableHair.includes(name) && Array.isArray(list) &&
            list.every(t => Number.isInteger(t) && t >= 0 && t < 1e6)))
        return reply(400, { error: '헤어 브러시 값이 올바르지 않습니다.' })
      const saved = await loadHelmetHairBrush()
      const lists = Object.entries(hair).filter(([, list]) => list.length)
        .map(([name, list]) => [name, [...new Set(list)].sort((a, b) => a - b)])
      if (lists.length) saved.helmets[input.helmet] = Object.fromEntries(lists)
      else delete saved.helmets[input.helmet]
      await writeFile(new URL(helmetHairBrushPath.href + '.tmp'), JSON.stringify(saved, null, 2) + '\n')
      await rename(new URL(helmetHairBrushPath.href + '.tmp'), helmetHairBrushPath)
      reply(200, { saved: true })
    })().catch(next)
  }
}
