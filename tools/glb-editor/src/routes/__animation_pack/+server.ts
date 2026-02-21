import { json } from '@sveltejs/kit'
import type { RequestHandler } from './$types'
import fs from 'node:fs/promises'
import path from 'node:path'
import {
  animationsDir,
  sanitizePackFileName,
} from '$lib/server/animation-pack-files'

function resolvePackFilePath(fileName: string): string {
  return path.join(animationsDir, fileName)
}

export const GET: RequestHandler = async ({ url }) => {
  const safeFileName = sanitizePackFileName(url.searchParams.get('file'))
  if (!safeFileName) {
    return new Response('Invalid file query', { status: 400 })
  }

  const filePath = resolvePackFilePath(safeFileName)
  try {
    const stat = await fs.stat(filePath)
    if (!stat.isFile()) {
      return new Response('Not found', { status: 404 })
    }

    const buffer = await fs.readFile(filePath)
    return new Response(buffer, {
      status: 200,
      headers: {
        'Content-Type': 'model/gltf-binary',
        'Content-Length': String(buffer.byteLength),
      },
    })
  } catch {
    return new Response('Not found', { status: 404 })
  }
}

export const POST: RequestHandler = async ({ request, url }) => {
  const safeFileName = sanitizePackFileName(url.searchParams.get('file'))
  if (!safeFileName) {
    return new Response('Invalid file query', { status: 400 })
  }

  try {
    const body = Buffer.from(await request.arrayBuffer())
    if (body.byteLength === 0) {
      return new Response('Empty body', { status: 400 })
    }

    await fs.mkdir(animationsDir, { recursive: true })
    await fs.writeFile(resolvePackFilePath(safeFileName), body)

    return json({
      ok: true,
      fileName: safeFileName,
    })
  } catch (error) {
    return json({ error: String(error) }, { status: 500 })
  }
}
