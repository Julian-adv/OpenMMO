import { constants as fsConstants } from 'node:fs'
import fs from 'node:fs/promises'
import path from 'node:path'

export interface AnimationPackEntry {
  packName: string
  fileName: string
}

export const animationsDir = path.resolve(
  process.cwd(),
  '../../client/public/models/animations'
)

export async function scanAnimationPackFiles(): Promise<AnimationPackEntry[]> {
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

export function sanitizePackFileName(file: string | null): string | null {
  if (!file) return null

  const base = path.basename(file)
  if (base !== file) return null
  if (!base.toLowerCase().endsWith('.glb')) return null
  return base
}
