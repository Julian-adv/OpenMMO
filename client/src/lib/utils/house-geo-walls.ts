/**
 * house-geo-walls.ts — Wall segment generation with door/window openings.
 */
import * as THREE from 'three'
import type { RoomData, WallConfig } from '../types/housing'
import { getHousingMaterial } from './housing-textures'
import {
  WALL_THICKNESS,
  FLOOR_THICKNESS,
  HOUSING_TEXTURES,
  WALL_DIR_INFO,
  bakedGeo,
  floorYBase,
  type WallDirection,
  type GeoEntry,
  type DoorMeshInfo,
} from './house-geo-utils'

const DOOR_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/wood_shutter_1k'
)
const DOOR_WIDTH = 1.0
const DOOR_HEIGHT = 2.2
const WINDOW_WIDTH = 1.0
const WINDOW_HEIGHT = 1.0
const WINDOW_BOTTOM = 1.2

/** Render 1m wall segments along a wall direction. */
export function collectWallSegments(
  segments: WallConfig[],
  dir: WallDirection,
  room: RoomData,
  roomIndex: number,
  frontEntries: GeoEntry[],
  backEntries: GeoEntry[],
  doors: DoorMeshInfo[]
) {
  const dirInfo = WALL_DIR_INFO[dir]
  const target = dirInfo.isFront ? frontEntries : backEntries
  const wh = room.wallHeight
  const yBase = floorYBase(room.floorLevel, wh) + FLOOR_THICKNESS / 2
  const { localX, localZ, sizeX, sizeZ } = room

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i]
    if (seg.variant === 'open') continue

    const texIdx = seg.texture % HOUSING_TEXTURES.length

    // Position: center of this 1m segment along the wall
    const segCenter = i + 0.5 // 0.5, 1.5, 2.5, ...
    let x: number, z: number, rotY: number

    const halfT = WALL_THICKNESS / 2
    switch (dir) {
      case 'north': {
        x = localX + segCenter
        z = localZ + halfT
        rotY = 0
        break
      }
      case 'south': {
        x = localX + segCenter
        z = localZ + sizeZ - halfT
        rotY = 0
        break
      }
      case 'east': {
        x = localX + sizeX - halfT
        z = localZ + segCenter
        rotY = Math.PI / 2
        break
      }
      case 'west': {
        x = localX + halfT
        z = localZ + segCenter
        rotY = Math.PI / 2
        break
      }
    }

    if (seg.variant === 'solid') {
      target.push({
        geo: bakedGeo(
          new THREE.BoxGeometry(1, wh, WALL_THICKNESS),
          x,
          yBase + wh / 2,
          z,
          rotY,
          1,
          wh
        ),
        textureIndex: texIdx,
      })
    } else {
      // door or window — opening centered in the 1m segment
      const openW = seg.variant === 'door' ? DOOR_WIDTH : WINDOW_WIDTH
      const openH = seg.variant === 'door' ? DOOR_HEIGHT : WINDOW_HEIGHT
      const openBot = seg.variant === 'door' ? 0 : WINDOW_BOTTOM
      const sideW = (1 - openW) / 2

      // Left and right solid strips
      if (sideW > 0.01) {
        for (const sign of [-1, 1]) {
          const offset = sign * (0.5 - sideW / 2)
          const sx = dir === 'north' || dir === 'south' ? x + offset : x
          const sz = dir === 'east' || dir === 'west' ? z + offset : z
          const uOffX = sign === -1 ? 0 : 1 - sideW
          target.push({
            geo: bakedGeo(
              new THREE.BoxGeometry(sideW, wh, WALL_THICKNESS),
              sx,
              yBase + wh / 2,
              sz,
              rotY,
              sideW,
              wh,
              uOffX,
              0
            ),
            textureIndex: texIdx,
          })
        }
      }

      // Bottom strip (windows)
      if (openBot > 0.01) {
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(openW, openBot, WALL_THICKNESS),
            x,
            yBase + openBot / 2,
            z,
            rotY,
            openW,
            openBot,
            sideW,
            0
          ),
          textureIndex: texIdx,
        })
      }

      // Top strip
      const topH = wh - openBot - openH
      if (topH > 0.01) {
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(openW, topH, WALL_THICKNESS),
            x,
            yBase + openBot + openH + topH / 2,
            z,
            rotY,
            openW,
            topH,
            sideW,
            openBot + openH
          ),
          textureIndex: texIdx,
        })
      }

      // Door panel mesh with hinge pivot
      if (seg.variant === 'door') {
        const panelGeo = new THREE.BoxGeometry(
          DOOR_WIDTH,
          DOOR_HEIGHT,
          WALL_THICKNESS
        )
        const panel = new THREE.Mesh(
          panelGeo,
          getHousingMaterial(DOOR_TEXTURE_IDX >= 0 ? DOOR_TEXTURE_IDX : texIdx)
        )
        panel.castShadow = true

        // Offset panel so its left edge is at the pivot (hinge on left side)
        panel.position.set(DOOR_WIDTH / 2, DOOR_HEIGHT / 2, 0)

        // Pivot group at the left edge of the door opening, at floor level
        const pivot = new THREE.Group()
        pivot.name = `door_r${roomIndex}_${dir}_${i}`

        // Position pivot at the hinge edge (left side of opening in local wall space)
        const hingeOffset = -DOOR_WIDTH / 2
        if (dir === 'north' || dir === 'south') {
          pivot.position.set(x + hingeOffset, yBase, z)
        } else {
          pivot.position.set(x, yBase, z + hingeOffset)
          pivot.rotation.y = Math.PI / 2
        }

        pivot.add(panel)

        const isOpen = seg.isOpen ?? false
        if (isOpen) {
          pivot.rotation.y += -Math.PI / 2
        }

        doors.push({
          pivot,
          roomIndex,
          wallDir: dir,
          segmentIndex: i,
          floorLevel: room.floorLevel,
          isOpen,
        })
      }
    }
  }
}
