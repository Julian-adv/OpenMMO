import * as THREE from 'three'
import { getHousingMaterial, HOUSING_TEXTURES } from './housing-textures'
import { WOOD_TEXTURE_IDX } from './house-geo-utils'
import { disposeObjectTree } from './disposeObjectTree'
import { networkManager } from '../network/socket'
import {
  BOARD_FACE_H,
  BOARD_FACE_W,
  noticeBoardKey,
  noticeBoardPapers,
  paperTilt,
  PAPER_SCALE,
  PAPER_H,
  PAPER_W,
  type NoticeBoardRef,
  type NoticePaper,
} from '../stores/noticeBoardStore'

const FACE_Y = 1.5
const PAPER_Z = 0.032
/** Each paper sits a hair in front of the one before it. */
const LAYER_Z = 0.0008
const PEEK_EVERY_MS = 60_000

let resources: ReturnType<typeof createResources> | null = null

function createResources() {
  const material = (color: number, roughness: number) =>
    new THREE.MeshStandardMaterial({ color, roughness })
  return {
    paper: material(0xe8dcc0, 0.95),
    oldPaper: material(0xc9b07a, 0.95),
    picture: material(0x6e5a44, 0.9),
    pin: material(0x8a2a1c, 0.6),
    sheet: new THREE.PlaneGeometry(PAPER_W, PAPER_H),
    pictureGeo: new THREE.PlaneGeometry(PAPER_W * 0.78, PAPER_H * 0.52),
    pinGeo: new THREE.SphereGeometry(0.018, 6, 4),
  }
}

function woodMaterial(glb: string): THREE.MeshStandardMaterial {
  const index = HOUSING_TEXTURES.findIndex((entry) => entry.glb === glb)
  return getHousingMaterial(index < 0 ? WOOD_TEXTURE_IDX : index)
}

function part(
  geometry: THREE.BufferGeometry,
  material: THREE.Material,
  x: number,
  y: number,
  z: number
): THREE.Mesh {
  const mesh = new THREE.Mesh(geometry, material)
  mesh.position.set(x, y, z)
  mesh.castShadow = true
  mesh.receiveShadow = true
  mesh.userData.isSignBoard = true
  return mesh
}

/** Village notice board, readable face on +Z, origin at ground centre. */
export function buildNoticeBoard(): THREE.Group {
  const frame = woodMaterial('housing/dark_wooden_planks_1k')
  const board = woodMaterial('housing/wood_plank_wall_1k')
  const group = new THREE.Group()
  group.name = 'notice-board'

  const post = new THREE.BoxGeometry(0.12, 2.6, 0.12)
  group.add(part(post, frame, -0.86, 1.3, 0))
  group.add(part(post, frame, 0.86, 1.3, 0))
  group.add(
    part(
      new THREE.BoxGeometry(BOARD_FACE_W, BOARD_FACE_H, 0.06),
      board,
      0,
      FACE_Y,
      0
    )
  )
  const rail = new THREE.BoxGeometry(1.8, 0.08, 0.1)
  group.add(part(rail, frame, 0, 2.24, 0.01))
  group.add(part(rail, frame, 0, 0.76, 0.01))

  const roof = new THREE.BoxGeometry(2.1, 0.05, 0.42)
  const front = part(roof, frame, 0, 2.58, 0.15)
  front.rotation.x = 0.45
  const back = part(roof, frame, 0, 2.58, -0.15)
  back.rotation.x = -0.45
  group.add(front, back)
  return group
}

/** Papers in board-local coordinates, oldest first so newer ones lie on top. */
function buildPapers(papers: NoticePaper[]): THREE.Group {
  const r = (resources ??= createResources())
  const group = new THREE.Group()
  group.name = 'notice-papers'
  papers.forEach((paper, i) => {
    const x = (paper.x / PAPER_SCALE - 0.5) * BOARD_FACE_W
    const y = FACE_Y + (0.5 - paper.y / PAPER_SCALE) * BOARD_FACE_H
    const z = PAPER_Z + i * LAYER_Z
    const sheet = new THREE.Group()
    sheet.position.set(x, y, z)
    sheet.rotation.z = paperTilt(paper.id)
    const face = part(r.sheet, paper.expired ? r.oldPaper : r.paper, 0, 0, 0)
    face.castShadow = false
    sheet.add(face)
    if (paper.poster) {
      const picture = part(r.pictureGeo, r.picture, 0, PAPER_H * 0.12, 0.0004)
      picture.castShadow = false
      sheet.add(picture)
    }
    sheet.add(part(r.pinGeo, r.pin, 0, PAPER_H / 2 - 0.04, 0.008))
    group.add(sheet)
  })
  return group
}

/** Placed boards showing papers, by object id. */
/** Keyed by `noticeBoardKey`; `refs` keeps what to peek for each key. */
const mounted = new Map<string, Set<THREE.Object3D>>()
const refs = new Map<string, NoticeBoardRef>()
const lastPeek = new Map<string, number>()
let latest = new Map<string, NoticePaper[]>()
let peekTimer: ReturnType<typeof setInterval> | null = null

function showPapers(root: THREE.Object3D, papers: NoticePaper[]) {
  const old = root.getObjectByName('notice-papers')
  if (old) {
    root.remove(old)
    disposeObjectTree(old)
  }
  root.add(buildPapers(papers))
}

function peek(key: string, force = false) {
  const board = refs.get(key)
  const now = Date.now()
  if (!board || (!force && now - (lastPeek.get(key) ?? 0) < PEEK_EVERY_MS))
    return
  lastPeek.set(key, now)
  networkManager.sendPeekNoticeBoard(board)
}

noticeBoardPapers.subscribe((all) => {
  for (const [id, papers] of all) {
    if (latest.get(id) === papers) continue
    for (const root of mounted.get(id) ?? []) showPapers(root, papers)
  }
  latest = all
})

/** Shows the board's real papers on a placed clone and keeps them current
 *  while the clone lives. */
export function mountNoticePapers(root: THREE.Object3D, board: NoticeBoardRef) {
  const key = noticeBoardKey(board)
  let roots = mounted.get(key)
  if (!roots) mounted.set(key, (roots = new Set()))
  refs.set(key, board)
  roots.add(root)
  showPapers(root, latest.get(key) ?? [])
  peek(key, !latest.has(key))
  root.addEventListener('dispose', () => {
    roots.delete(root)
    if (roots.size === 0) {
      mounted.delete(key)
      refs.delete(key)
    }
  })
  peekTimer ??= setInterval(() => {
    for (const id of mounted.keys()) peek(id)
  }, PEEK_EVERY_MS)
}
