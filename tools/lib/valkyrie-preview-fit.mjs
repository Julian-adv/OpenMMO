import { createHash } from 'node:crypto'
import { readFile, writeFile, rename, unlink, link } from 'node:fs/promises'

const root = new URL('../../', import.meta.url)
const target = new URL('doc/assets/valkyrie-bracer-preview-fit.json', root)
const part = 'assets/items/valkyrie_gloves/fitted_v2/bracers_female_valkyrie.glb'
const ranges = {
  outward: [-10, 10], height: [-15, 15], depth: [-10, 10], scale: [50, 180],
  'rotate-x': [-180, 180], 'rotate-y': [-180, 180], 'rotate-z': [-180, 180],
  'arm-outward': [-180, 180], 'arm-across': [-180, 180], 'arm-twist': [-180, 180],
  'move-arm-outward': [-15, 15], 'move-arm-across': [-15, 15], 'move-arm-length': [-15, 15],
  'left-move-outward': [-10, 10], 'left-move-across': [-10, 10], 'left-move-length': [-10, 10],
  'left-rotate-outward': [-90, 90], 'left-rotate-across': [-90, 90], 'left-rotate-length': [-90, 90],
}

export function valkyriePreviewFit(snapshot = target) {
  return (req, res, next) => {
    if (req.url?.split('?')[0] !== '/__female-preview/valkyrie-fit.json') return next()
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
      res.end(JSON.stringify(value))
    }
    void (async () => {
      if (req.method === 'GET') {
        try {
          reply(200, JSON.parse(await readFile(snapshot, 'utf8')))
        } catch (error) {
          if (error.code !== 'ENOENT') throw error
          reply(404, { error: '저장된 피팅이 없습니다.' })
        }
        return
      }
      if (req.method !== 'POST') return reply(405, { error: '지원하지 않는 요청입니다.' })
      if (req.headers['sec-fetch-site'] === 'cross-site' ||
          !req.headers['content-type']?.startsWith('application/json'))
        return reply(400, { error: '올바른 저장 요청이 아닙니다.' })
      let raw = ''
      for await (const chunk of req) {
        raw += chunk
        if (raw.length > 8192) return reply(413, { error: '저장값이 너무 큽니다.' })
      }
      let input
      try { input = JSON.parse(raw) } catch { return reply(400, { error: '저장값을 읽을 수 없습니다.' }) }
      if (!input || typeof input.hide_skin !== 'boolean' || typeof input.settings !== 'object' || !input.settings)
        return reply(400, { error: '피팅값이 올바르지 않습니다.' })
      const settings = {}
      for (const [key, [min, max]] of Object.entries(ranges)) {
        const value = input.settings[key]
        if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max)
          return reply(400, { error: `피팅값 범위를 확인하세요: ${key}` })
        settings[key] = value
      }
      const hash = async path => createHash('sha256').update(await readFile(new URL(path, root))).digest('hex')
      const result = {
        version: 1,
        saved_at: new Date().toISOString(),
        part_path: part,
        part_sha256: await hash(part),
        mask_sha256: await hash('client/src/previews/female-bracer-mask.ts'),
        hide_skin: input.hide_skin,
        settings,
      }
      const temporary = new URL(snapshot.href + `.${process.pid}.${Date.now()}.tmp`)
      await writeFile(temporary, JSON.stringify(result, null, 2) + '\n')
      try {
        if (input.automatic === true) {
          try { await link(temporary, snapshot) } catch (error) {
            if (error.code !== 'EEXIST') throw error
            return reply(200, { saved: false })
          }
        } else await rename(temporary, snapshot)
      } finally {
        await unlink(temporary).catch(error => { if (error.code !== 'ENOENT') throw error })
      }
      reply(200, { saved: true, saved_at: result.saved_at })
    })().catch(error => {
      serverError(error)
      if (!res.headersSent) reply(500, { error: '피팅 파일을 저장하지 못했습니다.' })
    })
  }
}

function serverError(error) {
  process.stderr.write(`Valkyrie fitting: ${error.message}\n`)
}
