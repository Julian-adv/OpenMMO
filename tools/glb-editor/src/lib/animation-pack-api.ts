const DEFAULT_ANIMATION_PACKS = [
  'locomotion',
  'combat_melee',
  'combat_ranged',
  'emote',
]
export const ANIMATION_PACK_PATH_HINT = 'client/public/models/animations'
const PACK_LIST_API = '/__animation_packs'
const PACK_FILE_API = '/__animation_pack'

interface AnimationPackEntry {
  packName: string
  fileName: string
}

interface AnimationPackListResponse {
  packs: AnimationPackEntry[]
}

export interface PackCatalog {
  packFilesByName: Record<string, string>
  animationPacks: string[]
}

function uniqueNames(items: string[]): string[] {
  const out: string[] = []
  for (const item of items) {
    const normalized = item.trim()
    if (!normalized || out.includes(normalized)) continue
    out.push(normalized)
  }
  return out
}

export async function loadAnimationPackCatalog(): Promise<PackCatalog> {
  const response = await fetch(PACK_LIST_API, { cache: 'no-store' })
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`)
  }

  const data = (await response.json()) as AnimationPackListResponse
  const nextMap: Record<string, string> = {}
  for (const pack of data.packs ?? []) {
    if (!pack?.packName || !pack?.fileName) continue
    nextMap[pack.packName.trim()] = pack.fileName.trim()
  }

  return {
    packFilesByName: nextMap,
    animationPacks: uniqueNames([
      ...Object.keys(nextMap),
      ...DEFAULT_ANIMATION_PACKS,
    ]),
  }
}

export function getDefaultPacks(): string[] {
  return [...DEFAULT_ANIMATION_PACKS]
}

export function getDefaultPackName(): string {
  return DEFAULT_ANIMATION_PACKS[0] ?? ''
}

export async function loadBasePackFile(
  packName: string,
  packFilesByName: Record<string, string>
): Promise<File | null> {
  const fileName = packFilesByName[packName]
  if (!fileName) return null

  const response = await fetch(
    `${PACK_FILE_API}?file=${encodeURIComponent(fileName)}`,
    {
      cache: 'no-store',
    }
  )
  if (!response.ok) {
    throw new Error(
      `기존 팩 파일 로드 실패: ${fileName} (HTTP ${response.status})`
    )
  }

  const blob = await response.blob()
  return new File([blob], fileName, { type: blob.type || 'model/gltf-binary' })
}

export async function savePackFileToAnimationsDir(
  fileName: string,
  arrayBuffer: ArrayBuffer
): Promise<void> {
  const response = await fetch(
    `${PACK_FILE_API}?file=${encodeURIComponent(fileName)}`,
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/octet-stream',
      },
      body: arrayBuffer,
    }
  )

  if (!response.ok) {
    throw new Error(`팩 파일 저장 실패: ${fileName} (HTTP ${response.status})`)
  }
}
