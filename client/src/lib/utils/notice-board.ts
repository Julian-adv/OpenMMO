import * as THREE from 'three'
import { getHousingMaterial, HOUSING_TEXTURES } from './housing-textures'
import { WOOD_TEXTURE_IDX } from './house-geo-utils'

const PAPERS: { x: number; y: number; w: number; h: number; tilt: number }[] = [
  { x: -0.5, y: 1.62, w: 0.36, h: 0.42, tilt: 0.05 },
  { x: -0.06, y: 1.66, w: 0.32, h: 0.3, tilt: -0.04 },
  { x: 0.42, y: 1.58, w: 0.4, h: 0.46, tilt: 0.03 },
  { x: -0.3, y: 1.18, w: 0.42, h: 0.32, tilt: -0.06 },
  { x: 0.18, y: 1.2, w: 0.3, h: 0.36, tilt: 0.07 },
]

let paperMaterial: THREE.MeshStandardMaterial | null = null
let pinMaterial: THREE.MeshStandardMaterial | null = null

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
  paperMaterial ??= new THREE.MeshStandardMaterial({
    color: 0xe8dcc0,
    roughness: 0.95,
  })
  pinMaterial ??= new THREE.MeshStandardMaterial({
    color: 0x8a2a1c,
    roughness: 0.6,
  })
  const frame = woodMaterial('housing/dark_wooden_planks_1k')
  const board = woodMaterial('housing/wood_plank_wall_1k')
  const group = new THREE.Group()
  group.name = 'notice-board'

  const post = new THREE.BoxGeometry(0.12, 2.3, 0.12)
  group.add(part(post, frame, -0.86, 1.15, 0))
  group.add(part(post, frame, 0.86, 1.15, 0))
  group.add(part(new THREE.BoxGeometry(1.66, 1.0, 0.06), board, 0, 1.42, 0))
  const rail = new THREE.BoxGeometry(1.8, 0.08, 0.1)
  group.add(part(rail, frame, 0, 1.94, 0.01))
  group.add(part(rail, frame, 0, 0.9, 0.01))

  const roof = new THREE.BoxGeometry(2.1, 0.05, 0.42)
  const front = part(roof, frame, 0, 2.28, 0.15)
  front.rotation.x = 0.45
  const back = part(roof, frame, 0, 2.28, -0.15)
  back.rotation.x = -0.45
  group.add(front, back)

  const pin = new THREE.SphereGeometry(0.018, 6, 4)
  for (const paper of PAPERS) {
    const sheet = part(
      new THREE.PlaneGeometry(paper.w, paper.h),
      paperMaterial,
      paper.x,
      paper.y,
      0.032
    )
    sheet.rotation.z = paper.tilt
    sheet.castShadow = false
    group.add(sheet)
    group.add(
      part(pin, pinMaterial, paper.x, paper.y + paper.h / 2 - 0.04, 0.04)
    )
  }
  return group
}
