#!/usr/bin/env node
/**
 * normalize-skeleton.mjs
 *
 * GLB 파일의 Armature 노드 회전을 스켈레톤 루트 본에 bake 해서
 * 좌표계를 Y-up 으로 정규화한다.
 *
 * 변환 내용:
 *   - Armature.rotation → identity (0, 0, 0, 1)
 *   - 루트 본 bind-pose translation = R_arm * old_translation
 *   - 루트 본 bind-pose rotation    = R_arm * old_rotation  (→ identity)
 *   - 모든 기존 애니메이션의 루트 본 position/rotation 트랙 보정
 *   - inverse bind matrices 는 world transform 이 보존되므로 수정 불필요
 *
 * Usage:
 *   node normalize-skeleton.mjs <input.glb> [output.glb]
 *
 * output.glb 미지정 시 → input_normalized.glb 로 저장
 */

import fs from 'fs'
import path from 'path'

// ─── Quaternion helpers (glTF convention: [x, y, z, w]) ───────────────────────

function qmul(a, b) {
  const [ax, ay, az, aw] = a
  const [bx, by, bz, bw] = b
  return [
    aw * bx + ax * bw + ay * bz - az * by,
    aw * by - ax * bz + ay * bw + az * bx,
    aw * bz + ax * by - ay * bx + az * bw,
    aw * bw - ax * bx - ay * by - az * bz,
  ]
}

function qnorm(q) {
  const len = Math.hypot(...q)
  return len < 1e-10 ? [0, 0, 0, 1] : q.map((v) => v / len)
}

/** 벡터 v 를 단위 쿼터니언 q 로 회전 */
function qrotate(q, v) {
  const vq = [v[0], v[1], v[2], 0]
  const qc = [-q[0], -q[1], -q[2], q[3]] // conjugate
  const r = qmul(qmul(q, vq), qc)
  return [r[0], r[1], r[2]]
}

/** 회전각이 threshold 도 미만이면 identity로 간주 */
function isIdentityQ(q, thresholdDeg = 0.01) {
  const [x, y, z, w] = q
  const angle = 2 * Math.acos(Math.min(1, Math.abs(w))) * (180 / Math.PI)
  return angle < thresholdDeg && Math.abs(x) < 1e-6 && Math.abs(y) < 1e-6 && Math.abs(z) < 1e-6
}

// ─── GLB parse / write ────────────────────────────────────────────────────────

function parseGlb(filePath) {
  const buf = fs.readFileSync(filePath)
  if (buf.readUInt32LE(0) !== 0x46546c67) throw new Error('GLB 파일이 아닙니다.')

  let offset = 12
  let json = null
  let binaryBuffer = null

  while (offset < buf.length) {
    const chunkLen = buf.readUInt32LE(offset)
    const chunkType = buf.readUInt32LE(offset + 4)
    const chunkData = buf.slice(offset + 8, offset + 8 + chunkLen)
    offset += 8 + chunkLen

    if (chunkType === 0x4e4f534a) {
      json = JSON.parse(chunkData.toString('utf8'))
    } else if (chunkType === 0x004e4942) {
      binaryBuffer = Buffer.from(chunkData) // 복사본
    }
  }

  if (!json) throw new Error('JSON 청크를 찾을 수 없습니다.')
  if (!binaryBuffer) throw new Error('BIN 청크를 찾을 수 없습니다.')
  return { json, binaryBuffer }
}

function writeGlb(filePath, json, binaryBuffer) {
  const jsonStr = JSON.stringify(json)
  // JSON 청크는 4바이트 정렬 (공백 패딩)
  const jsonPad = (4 - (jsonStr.length % 4)) % 4
  const jsonBytes = Buffer.concat([
    Buffer.from(jsonStr, 'utf8'),
    Buffer.alloc(jsonPad, 0x20),
  ])
  // BIN 청크는 4바이트 정렬 (0 패딩)
  const binPad = (4 - (binaryBuffer.length % 4)) % 4
  const binBytes = Buffer.concat([binaryBuffer, Buffer.alloc(binPad, 0)])

  const totalLength = 12 + 8 + jsonBytes.length + 8 + binBytes.length
  const out = Buffer.alloc(totalLength)

  out.writeUInt32LE(0x46546c67, 0) // 'glTF'
  out.writeUInt32LE(2, 4) // version
  out.writeUInt32LE(totalLength, 8)

  out.writeUInt32LE(jsonBytes.length, 12)
  out.writeUInt32LE(0x4e4f534a, 16) // JSON
  jsonBytes.copy(out, 20)

  const binStart = 20 + jsonBytes.length
  out.writeUInt32LE(binBytes.length, binStart)
  out.writeUInt32LE(0x004e4942, binStart + 4) // BIN
  binBytes.copy(out, binStart + 8)

  fs.writeFileSync(filePath, out)
}

// ─── Accessor read / write ────────────────────────────────────────────────────

const COMPONENT_COUNT = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 }

function readAccessorData(json, binaryBuffer, accessorIdx) {
  const acc = json.accessors[accessorIdx]
  const bv = json.bufferViews[acc.bufferView]
  const compCount = COMPONENT_COUNT[acc.type]
  const byteOffset = (bv.byteOffset ?? 0) + (acc.byteOffset ?? 0)
  const stride = bv.byteStride ?? compCount * 4

  const result = []
  for (let i = 0; i < acc.count; i++) {
    const base = byteOffset + i * stride
    const item = []
    for (let j = 0; j < compCount; j++) {
      item.push(binaryBuffer.readFloatLE(base + j * 4))
    }
    result.push(item)
  }
  return result
}

function writeAccessorData(json, binaryBuffer, accessorIdx, data) {
  const acc = json.accessors[accessorIdx]
  const bv = json.bufferViews[acc.bufferView]
  const compCount = COMPONENT_COUNT[acc.type]
  const byteOffset = (bv.byteOffset ?? 0) + (acc.byteOffset ?? 0)
  const stride = bv.byteStride ?? compCount * 4

  for (let i = 0; i < data.length; i++) {
    const base = byteOffset + i * stride
    for (let j = 0; j < compCount; j++) {
      binaryBuffer.writeFloatLE(data[i][j], base + j * 4)
    }
  }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

function main() {
  const args = process.argv.slice(2)
  if (args.length < 1) {
    console.error('Usage: node normalize-skeleton.mjs <input.glb> [output.glb]')
    process.exit(1)
  }

  const inputPath = args[0]
  const outputPath =
    args[1] ?? inputPath.replace(/\.glb$/i, '_normalized.glb')

  console.log(`입력: ${inputPath}`)
  console.log(`출력: ${outputPath}`)

  const { json, binaryBuffer } = parseGlb(inputPath)

  // 모든 스킨의 조인트 인덱스 수집 (bone 노드 판별용)
  const allJointIndices = new Set()
  for (const skin of json.skins ?? []) {
    for (const j of skin.joints ?? []) allJointIndices.add(j)
  }

  // Armature 노드 탐색:
  //   - allJointIndices 에 속하지 않는 노드
  //   - 비-identity 회전을 가짐
  //   - 직계 자식 중 조인트가 있음
  const armatureList = []
  for (let i = 0; i < json.nodes.length; i++) {
    if (allJointIndices.has(i)) continue
    const node = json.nodes[i]
    if (!node.rotation) continue
    if (isIdentityQ(node.rotation)) continue
    const hasJointChild = (node.children ?? []).some((ci) =>
      allJointIndices.has(ci)
    )
    if (hasJointChild) armatureList.push(i)
  }

  if (armatureList.length === 0) {
    console.log('정규화할 Armature 노드가 없습니다. 종료.')
    return
  }

  for (const armIdx of armatureList) {
    const armNode = json.nodes[armIdx]
    const armQ = armNode.rotation // [x, y, z, w]
    console.log(`\nArmature: "${armNode.name ?? armIdx}"  rot=[${armQ.map((v) => v.toFixed(5)).join(', ')}]`)

    // Armature 직계 자식 중 조인트인 노드 = 스켈레톤 루트
    const rootJointIndices = (armNode.children ?? []).filter((ci) =>
      allJointIndices.has(ci)
    )

    for (const rootIdx of rootJointIndices) {
      const rootNode = json.nodes[rootIdx]
      const rootName = rootNode.name ?? `node#${rootIdx}`
      console.log(`  루트 본: "${rootName}"`)

      // ── Bind pose 업데이트 ─────────────────────────────────────────────────
      const oldT = rootNode.translation ?? [0, 0, 0]
      const oldR = rootNode.rotation ?? [0, 0, 0, 1]

      const newT = qrotate(armQ, oldT)
      const newR = qnorm(qmul(armQ, oldR))

      console.log(
        `    translation: [${oldT.map((v) => v.toFixed(4)).join(', ')}]` +
          ` → [${newT.map((v) => v.toFixed(4)).join(', ')}]`
      )
      console.log(
        `    rotation:    [${oldR.map((v) => v.toFixed(4)).join(', ')}]` +
          ` → [${newR.map((v) => v.toFixed(4)).join(', ')}]`
      )

      rootNode.translation = newT
      rootNode.rotation = newR

      // ── 애니메이션 트랙 보정 ───────────────────────────────────────────────
      let updatedPos = 0
      let updatedRot = 0

      for (const anim of json.animations ?? []) {
        for (const ch of anim.channels ?? []) {
          if (ch.target.node !== rootIdx) continue
          const outputIdx = anim.samplers[ch.sampler].output

          if (ch.target.path === 'translation') {
            const data = readAccessorData(json, binaryBuffer, outputIdx)
            const updated = data.map((v) => qrotate(armQ, v))
            writeAccessorData(json, binaryBuffer, outputIdx, updated)
            updatedPos++
          } else if (ch.target.path === 'rotation') {
            const data = readAccessorData(json, binaryBuffer, outputIdx)
            const updated = data.map((q) => qnorm(qmul(armQ, q)))
            writeAccessorData(json, binaryBuffer, outputIdx, updated)
            updatedRot++
          }
        }
      }

      console.log(`    position 트랙 ${updatedPos}개 / rotation 트랙 ${updatedRot}개 업데이트`)
    }

    // ── Armature 회전 → identity ───────────────────────────────────────────
    armNode.rotation = [0, 0, 0, 1]
    console.log(`  Armature.rotation → identity`)

    // Armature 자체에 걸린 rotation 애니메이션 트랙도 identity로 초기화
    for (const anim of json.animations ?? []) {
      for (const ch of anim.channels ?? []) {
        if (ch.target.node !== armIdx || ch.target.path !== 'rotation') continue
        const outputIdx = anim.samplers[ch.sampler].output
        const data = readAccessorData(json, binaryBuffer, outputIdx)
        const identity = data.map(() => [0, 0, 0, 1])
        writeAccessorData(json, binaryBuffer, outputIdx, identity)
        console.log(`  Armature rotation 트랙 → identity (${anim.name ?? 'unnamed'})`)
      }
    }
  }

  writeGlb(outputPath, json, binaryBuffer)
  console.log(`\n✓ 저장 완료: ${path.resolve(outputPath)}`)
}

main()
