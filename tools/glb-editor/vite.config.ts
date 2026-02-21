import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import type { Connect, Plugin } from 'vite'
import { constants as fsConstants } from 'node:fs'
import fs from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const animationsDir = path.resolve(
  __dirname,
  '../../client/public/models/animations'
)

const PACK_LIST_PATH = '/__animation_packs'
const PACK_FILE_PATH = '/__animation_pack'

interface AnimationPackEntry {
  packName: string
  fileName: string
}

async function scanAnimationPackFiles(): Promise<AnimationPackEntry[]> {
  try {
    await fs.access(animationsDir, fsConstants.R_OK)
  } catch {
    return []
  }

  const entries = await fs.readdir(animationsDir, { withFileTypes: true })
  return entries
    .filter(
      (entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.glb')
    )
    .map((entry) => ({
      packName: entry.name.slice(0, -4),
      fileName: entry.name,
    }))
    .sort((a, b) => a.packName.localeCompare(b.packName))
}

function sanitizeFileQuery(file: string | null): string | null {
  if (!file) return null
  const base = path.basename(file)
  if (base !== file) return null
  if (!base.toLowerCase().endsWith('.glb')) return null
  return base
}

async function readRequestBody(req: Connect.IncomingMessage): Promise<Buffer> {
  const chunks: Buffer[] = []
  for await (const chunk of req) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk))
  }
  return Buffer.concat(chunks)
}

function animationPackApiMiddleware(): Connect.NextHandleFunction {
  return async (req, res, next) => {
    if (!req.method || !req.url) {
      return next()
    }

    const url = new URL(req.url, 'http://localhost')
    if (url.pathname === PACK_LIST_PATH) {
      try {
        const packs = await scanAnimationPackFiles()
        res.statusCode = 200
        res.setHeader('Content-Type', 'application/json; charset=utf-8')
        res.end(JSON.stringify({ packs }))
      } catch (error) {
        res.statusCode = 500
        res.setHeader('Content-Type', 'application/json; charset=utf-8')
        res.end(JSON.stringify({ error: String(error) }))
      }
      return
    }

    if (url.pathname === PACK_FILE_PATH && req.method === 'GET') {
      const safeFileName = sanitizeFileQuery(url.searchParams.get('file'))
      if (!safeFileName) {
        res.statusCode = 400
        res.end('Invalid file query')
        return
      }

      const filePath = path.join(animationsDir, safeFileName)
      try {
        const stat = await fs.stat(filePath)
        if (!stat.isFile()) {
          res.statusCode = 404
          res.end('Not found')
          return
        }

        const buffer = await fs.readFile(filePath)
        res.statusCode = 200
        res.setHeader('Content-Type', 'model/gltf-binary')
        res.setHeader('Content-Length', String(buffer.byteLength))
        res.end(buffer)
      } catch {
        res.statusCode = 404
        res.end('Not found')
      }
      return
    }

    if (url.pathname === PACK_FILE_PATH && req.method === 'POST') {
      const safeFileName = sanitizeFileQuery(url.searchParams.get('file'))
      if (!safeFileName) {
        res.statusCode = 400
        res.end('Invalid file query')
        return
      }

      try {
        const body = await readRequestBody(req)
        if (body.byteLength === 0) {
          res.statusCode = 400
          res.end('Empty body')
          return
        }

        await fs.mkdir(animationsDir, { recursive: true })
        const filePath = path.join(animationsDir, safeFileName)
        await fs.writeFile(filePath, body)

        res.statusCode = 200
        res.setHeader('Content-Type', 'application/json; charset=utf-8')
        res.end(
          JSON.stringify({
            ok: true,
            fileName: safeFileName,
          })
        )
      } catch (error) {
        res.statusCode = 500
        res.setHeader('Content-Type', 'application/json; charset=utf-8')
        res.end(JSON.stringify({ error: String(error) }))
      }
      return
    }

    return next()
  }
}

function animationPackApiPlugin(): Plugin {
  return {
    name: 'animation-pack-api',
    configureServer(server) {
      server.middlewares.use(animationPackApiMiddleware())
    },
    configurePreviewServer(server) {
      server.middlewares.use(animationPackApiMiddleware())
    },
  }
}

export default defineConfig({
  plugins: [svelte(), animationPackApiPlugin()],
})
