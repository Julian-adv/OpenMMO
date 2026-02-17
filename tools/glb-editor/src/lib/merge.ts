import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { GLTFExporter } from 'three/examples/jsm/exporters/GLTFExporter.js'

export type RotationFixAxis = 'x' | 'y' | 'z'
export type RotationFixScope = 'root' | 'all'
export type RotationFixOrder = 'pre' | 'post'

export interface RotationFixOptions {
  enabled: boolean
  axis: RotationFixAxis
  deg: number
  scope: RotationFixScope
  order: RotationFixOrder
}

export interface MergeOptions {
  animName: string
  rotationFix: RotationFixOptions
  selectedBClipIndex: number
}

export interface MergeStats {
  totalTracks: number
  mappedTracks: number
  correctedTracks: number
  mergedClipCount: number
}

export interface MergeOutput {
  merged: Blob
  stats: MergeStats
}

interface MatchResult {
  name: string
  score: number
  reason: string
}

interface ANodeIndex {
  idx: Map<string, Set<string>>
  originals: Set<string>
}

interface RotationFixConfig {
  axis: RotationFixAxis
  deg: number
  scope: RotationFixScope
  order: RotationFixOrder
  q: THREE.Quaternion
}

const exporter = new GLTFExporter()

export async function mergeAnimationsIntoA(
  gltfA: GLTF,
  gltfB: GLTF,
  options: MergeOptions,
  log: (message: string) => void
): Promise<MergeOutput> {
  const aIndex = buildANodeIndex(gltfA.scene)
  log(`a.glb 노드 수(이름 보유): ${aIndex.originals.size}`)

  const takenNames = new Set<string>()
  for (const clip of gltfA.animations ?? []) {
    const old = clip.name && clip.name.trim() ? clip.name : 'Animation'
    uniqueName(old, takenNames)
  }

  const rotFix = buildRotationFixConfig(options.rotationFix)
  if (rotFix) {
    log(
      `회전 보정 활성화: axis=${rotFix.axis}, deg=${rotFix.deg}, scope=${rotFix.scope}, order=${rotFix.order}`
    )
  }

  const mappedBAnims: THREE.AnimationClip[] = []
  let totalTracks = 0
  let mappedTracks = 0
  let correctedTracks = 0

  const bAnimations = gltfB.animations ?? []
  const srcClip = bAnimations[options.selectedBClipIndex]
  if (!srcClip) {
    log('b.glb에서 병합할 애니메이션이 없습니다.')
    throw new Error('선택된 B 클립이 없습니다.')
  }
  log(
    `b.glb 선택 클립 병합: index=${options.selectedBClipIndex} ("${srcClip.name}")`
  )
  const clipsToMerge = [srcClip]

  for (const clip of clipsToMerge) {
    const newTracks: THREE.KeyframeTrack[] = []
    let clipCorrected = 0

    for (const track of clip.tracks) {
      totalTracks += 1
      const bNode = track.name.split('.')[0]
      const match = pickBestMatch(bNode, aIndex)

      if (!match) {
        log(`매치 실패: ${bNode}`)
        continue
      }

      const cloned = track.clone()
      cloned.name = cloned.name.replace(bNode, match.name)

      if (
        rotFix &&
        isQuaternionTrack(cloned) &&
        (rotFix.scope === 'all' || isLikelyRootNode(match.name))
      ) {
        applyQuaternionRotationFix(cloned, rotFix)
        clipCorrected += 1
        correctedTracks += 1
      }

      newTracks.push(cloned)
      mappedTracks += 1
      log(
        `매핑: ${bNode} -> ${match.name} (reason=${match.reason}, score=${match.score.toFixed(3)})`
      )
    }

    if (newTracks.length === 0) {
      log(`클립 "${clip.name}" 매핑된 트랙 없음 -> 스킵`)
      continue
    }

    const newClip = clip.clone()
    newClip.tracks = newTracks

    newClip.name = uniqueName(options.animName, takenNames)

    mappedBAnims.push(newClip)
    log(
      `클립 "${clip.name}" -> "${newClip.name}" (${newTracks.length} 트랙 매핑 성공)`
    )

    if (clipCorrected > 0) {
      log(`회전 보정 적용 트랙: ${clipCorrected}`)
    }
  }

  const mergedAnims = [...(gltfA.animations ?? []), ...mappedBAnims]
  log(`요약: 총 트랙 ${totalTracks}개 중 ${mappedTracks}개 매핑 성공`)
  if (rotFix) {
    log(`요약: 회전 보정 적용 트랙 ${correctedTracks}개`)
  }
  log(`최종 병합 애니메이션 개수: ${mergedAnims.length}`)

  const result = await exportBinaryGLB(gltfA.scene, mergedAnims)
  const merged = new Blob([result], { type: 'model/gltf-binary' })

  return {
    merged,
    stats: {
      totalTracks,
      mappedTracks,
      correctedTracks,
      mergedClipCount: mergedAnims.length,
    },
  }
}

function exportBinaryGLB(
  scene: THREE.Object3D,
  animations: THREE.AnimationClip[]
): Promise<ArrayBuffer> {
  return new Promise((resolve, reject) => {
    exporter.parse(
      scene,
      (result) => {
        if (result instanceof ArrayBuffer) {
          resolve(result)
          return
        }
        reject(
          new Error('GLB binary export failed: non-binary result returned')
        )
      },
      (err) => {
        reject(err instanceof Error ? err : new Error(String(err)))
      },
      {
        binary: true,
        animations,
      }
    )
  })
}

function normalizeSide(name: string): string {
  let n = name.replace(/\.l\b/gi, '_l').replace(/\.r\b/gi, '_r')
  n = n.replace(/[\s-]+l\b/gi, '_l').replace(/[\s-]+r\b/gi, '_r')
  return n
}

function stripNumericSuffix(name: string): string {
  let n = name
  for (let i = 0; i < 3; i += 1) {
    n = n.replace(/([_.\s-]?\(?\d{1,4}\)?)$/, '')
  }
  return n
}

function canonicalKey(name: string): string {
  if (!name) return ''
  let n = normalizeSide(name)
  n = stripNumericSuffix(n)
  n = n.toLowerCase()
  n = n.replace(/[^a-z0-9_]/g, '')
  n = n.replace(/_+/g, '_').replace(/^_+|_+$/g, '')
  return n
}

function levenshtein(a: string, b: string): number {
  const m = a.length
  const n = b.length
  if (m === 0) return n
  if (n === 0) return m

  const dp = new Array<number>(n + 1)
  for (let j = 0; j <= n; j += 1) dp[j] = j

  for (let i = 1; i <= m; i += 1) {
    let prev = dp[0]
    dp[0] = i

    for (let j = 1; j <= n; j += 1) {
      const temp = dp[j]
      const cost = a[i - 1] === b[j - 1] ? 0 : 1
      dp[j] = Math.min(dp[j] + 1, dp[j - 1] + 1, prev + cost)
      prev = temp
    }
  }

  return dp[n]
}

function similarity(a: string, b: string): number {
  if (!a || !b) return 0
  const d = levenshtein(a, b)
  const L = Math.max(a.length, b.length)
  return 1 - d / (L || 1)
}

function buildANodeIndex(scene: THREE.Object3D): ANodeIndex {
  const idx = new Map<string, Set<string>>()
  const originals = new Set<string>()

  scene.traverse((obj) => {
    if (!obj.name) return

    const raw = obj.name
    originals.add(raw)
    const key = canonicalKey(raw)
    if (!key) return

    if (!idx.has(key)) {
      idx.set(key, new Set<string>())
    }

    idx.get(key)?.add(raw)
  })

  return { idx, originals }
}

function pickBestMatch(
  bNodeRaw: string,
  aIndex: ANodeIndex
): MatchResult | null {
  const bKey = canonicalKey(bNodeRaw)

  if (aIndex.idx.has(bKey)) {
    const cands = Array.from(aIndex.idx.get(bKey) ?? [])
    let best = cands[0]
    let bestScore = -1

    for (const cand of cands) {
      const score = similarity(canonicalKey(cand), bKey)
      if (score > bestScore) {
        bestScore = score
        best = cand
      }
    }

    return { name: best, score: 1.0, reason: 'exact-key' }
  }

  const bStripped = canonicalKey(stripNumericSuffix(normalizeSide(bNodeRaw)))
  if (aIndex.idx.has(bStripped)) {
    const cands = Array.from(aIndex.idx.get(bStripped) ?? [])
    return { name: cands[0], score: 0.98, reason: 'stripped-suffix' }
  }

  const bSideNorm = canonicalKey(normalizeSide(bNodeRaw))
  if (aIndex.idx.has(bSideNorm)) {
    const cands = Array.from(aIndex.idx.get(bSideNorm) ?? [])
    return { name: cands[0], score: 0.96, reason: 'side-normalized' }
  }

  const threshold = 0.82
  let bestName: string | null = null
  let bestScore = 0

  for (const [aKey, setRaw] of aIndex.idx.entries()) {
    const score = similarity(bKey, aKey)
    if (score > bestScore) {
      bestScore = score
      bestName = setRaw.values().next().value ?? null
    }
  }

  if (bestName && bestScore >= threshold) {
    return { name: bestName, score: bestScore, reason: 'fuzzy' }
  }

  return null
}

function isQuaternionTrack(
  track: THREE.KeyframeTrack
): track is THREE.QuaternionKeyframeTrack {
  return (
    track.name.endsWith('.quaternion') &&
    track.values.length % 4 === 0 &&
    track instanceof THREE.QuaternionKeyframeTrack
  )
}

function isLikelyRootNode(name: string): boolean {
  const key = canonicalKey(name).replace(/_/g, '')
  return (
    key === 'root' ||
    key === 'armature' ||
    key === 'hips' ||
    key === 'pelvis' ||
    key === 'mixamorighips' ||
    key === 'skeletonroot' ||
    key.endsWith('root')
  )
}

function buildRotationFixConfig(
  options: RotationFixOptions
): RotationFixConfig | null {
  if (!options.enabled) return null
  if (!Number.isFinite(options.deg) || Math.abs(options.deg) < 1e-8) return null

  const q = new THREE.Quaternion()
  const rad = THREE.MathUtils.degToRad(options.deg)

  if (options.axis === 'x') q.setFromAxisAngle(new THREE.Vector3(1, 0, 0), rad)
  else if (options.axis === 'y')
    q.setFromAxisAngle(new THREE.Vector3(0, 1, 0), rad)
  else q.setFromAxisAngle(new THREE.Vector3(0, 0, 1), rad)

  return {
    axis: options.axis,
    deg: options.deg,
    scope: options.scope,
    order: options.order,
    q,
  }
}

function applyQuaternionRotationFix(
  track: THREE.QuaternionKeyframeTrack,
  fixCfg: RotationFixConfig
): void {
  const q = new THREE.Quaternion()
  const values = track.values

  for (let i = 0; i < values.length; i += 4) {
    q.set(values[i], values[i + 1], values[i + 2], values[i + 3])

    if (fixCfg.order === 'post') q.multiply(fixCfg.q)
    else q.premultiply(fixCfg.q)

    q.normalize()
    values[i] = q.x
    values[i + 1] = q.y
    values[i + 2] = q.z
    values[i + 3] = q.w
  }
}

function uniqueName(base: string, taken: Set<string>): string {
  const raw = base && base.trim() ? base.trim() : 'Clip'
  if (!taken.has(raw)) {
    taken.add(raw)
    return raw
  }

  let i = 1
  while (taken.has(`${raw}_${i}`)) i += 1

  const finalName = `${raw}_${i}`
  taken.add(finalName)
  return finalName
}
