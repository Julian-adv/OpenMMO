import { readFile, stat } from 'node:fs/promises'
import { sources, output } from './lib/female-preview.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'

const pending = new Map()

export function femalePreviewPlugin() {
  return {
    name: 'female-preview-assets',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const match = /^\/__female-preview\/(base|bob|knight|valkyrie)\.glb$|^\/__female-preview\/(animations)\.json$/.exec(req.url?.split('?')[0] ?? '')
        if (req.method !== 'GET' || !match) return next()
        const name = match[1] ?? match[2]
        if (!Object.hasOwn(sources, name)) return next()
        if (!pending.has(name)) pending.set(name, ensureFemalePreview(name).finally(() => pending.delete(name)))
        void pending.get(name).then(async () => {
          const compressed = name === 'animations' && /\bgzip\b/.test(req.headers['accept-encoding'] ?? '')
          const target = compressed ? new URL(output(name).href + '.gz') : output(name)
          const info = await stat(target)
          const etag = `W/"${info.size}-${info.mtimeMs}"`
          res.setHeader('Cache-Control', 'private, no-cache')
          res.setHeader('ETag', etag)
          if (name === 'animations') res.setHeader('Vary', 'Accept-Encoding')
          if (req.headers['if-none-match'] === etag) {
            res.statusCode = 304
            res.end()
            return
          }
          const bytes = await readFile(target)
          res.setHeader('Content-Type', name === 'animations' ? 'application/json' : 'model/gltf-binary')
          if (compressed) res.setHeader('Content-Encoding', 'gzip')
          res.setHeader('Content-Length', bytes.length)
          res.end(bytes)
        }).catch(next)
      })
    },
  }
}
