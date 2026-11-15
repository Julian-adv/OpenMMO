/**
 * house-geo-walls.ts — Wall segment generation with door/window openings.
 */
import * as THREE from 'three'
import type { RoomData, WallConfig } from '../types/housing'
import { DECOR_UV, getHousingMaterial } from './housing-textures'
import {
  crossRoomDoorPartner,
  doubleDoorPartner,
  facingSegmentRef,
  getWallByDir,
  isDoorVariant,
  wallLineCoord,
} from '../managers/housing-passability'
import {
  WALL_THICKNESS,
  FLOOR_THICKNESS,
  HOUSING_TEXTURES,
  FRAME_DEPTH,
  WOOD_TEXTURE_IDX,
  SHUTTER_PANEL_TEXTURE_IDX,
  WALL_DIR_INFO,
  bakedGeo,
  floorYBase,
  floorOverhang,
  type WallDirection,
  interiorWall,
  type GeoEntry,
  type FloorEntries,
} from './house-geo-utils'
import { STYLE_TEX, type ResolvedHouseStyle } from './house-style'

const DOOR_TEXTURE_IDX = WOOD_TEXTURE_IDX
const DOOR_WIDTH = 0.8
const DOOR_HEIGHT = 2.2
const WINDOW_WIDTH = 0.8
const WINDOW_HEIGHT = 1.0
const WINDOW_BOTTOM = 1.2
const FRAME_BEAM_FRAC = 0.05 // beam thickness as fraction of wall height
const FRAME_BEAM_Y_FRAC = 0.4 // beam position from bottom
const FRAME_SIDE_FRAC = 0.1 // side strip width as fraction of segment width
const FRAME_BOTTOM_FRAC = 0.05 // bottom strip height fraction
const FRAME_DIAG_THICKNESS = 0.06 // diagonal beam thickness in meters
const SHUTTER_BORDER = 0.045 // shutter border frame thickness (used for side-face UV)
const TOP_PLATE_HEIGHT = 0.1
const PLINTH_HEIGHT = 0.3
const PLINTH_DROP = 0.15
const PLINTH_DEPTH = FRAME_DEPTH + 0.06
const SILL_WIDTH = WINDOW_WIDTH + 0.2
const SILL_HEIGHT = 0.05
const SILL_DEPTH = 0.12
const SHUTTER_WIDTH = 0.24
const SHUTTER_THICKNESS = 0.03
const FLOWER_BOX_WIDTH = WINDOW_WIDTH - 0.06
const FLOWER_BOX_HEIGHT = 0.12
const FLOWER_BOX_DEPTH = 0.14
const FLOWER_HEIGHT = 0.3
const LANTERN_REACH = 0.22
const SURROUND_JAMB = 0.07
const DOOR_STEP_HEIGHT = 0.14
const DOOR_STEP_DEPTH = 0.4
const IVY_WIDTH = 1.1
const IVY_HEIGHT = 2.2

function addPlinth(
  target: GeoEntry[],
  w: number,
  x: number,
  yBase: number,
  z: number,
  rotY: number,
  uOffset = 0
) {
  const h = PLINTH_HEIGHT + PLINTH_DROP
  target.push({
    geo: bakedGeo(
      new THREE.BoxGeometry(w, h, PLINTH_DEPTH),
      x,
      yBase + PLINTH_HEIGHT - h / 2,
      z,
      rotY,
      w,
      h,
      uOffset
    ),
    textureIndex: STYLE_TEX.stone,
  })
}

/** Covers the floor slab edge between storeys. */
function addJettyBeam(
  target: GeoEntry[],
  w: number,
  x: number,
  yBase: number,
  z: number,
  rotY: number
) {
  target.push({
    geo: bakedGeo(
      new THREE.BoxGeometry(w, JETTY_BEAM_HEIGHT, JETTY_BEAM_DEPTH),
      x,
      yBase - FLOOR_THICKNESS / 2,
      z,
      rotY,
      w,
      JETTY_BEAM_HEIGHT
    ),
    textureIndex: STYLE_TEX.frame,
  })
}

/** Stable per-piece choice within a house. */
function pieceHash(seed: number, a: number, b: number, c: number): number {
  let h =
    seed ^
    Math.imul(a + 1, 73856093) ^
    Math.imul(b + 1, 19349663) ^
    Math.imul(c + 1, 83492791)
  h = Math.imul(h ^ (h >>> 13), 0x5bd1e995)
  return (h ^ (h >>> 15)) >>> 0
}
const OUTWARD_ROT_Y: Record<WallDirection, number> = {
  south: 0,
  north: Math.PI,
  east: Math.PI / 2,
  west: -Math.PI / 2,
}
const JETTY_BEAM_HEIGHT = FLOOR_THICKNESS + 0.04
const JETTY_BEAM_DEPTH = FRAME_DEPTH + 0.03

/** 'x' crosses the panel; a sign is one brace rising toward that local side. */
type FrameBrace = 'x' | 'none' | -1 | 1
const WINDOW_CLICK_MATERIAL = new THREE.MeshBasicMaterial({
  side: THREE.DoubleSide,
})

// Shared temp matrix for frame diagonal geometry (single-threaded, sync only)
const _frameMat = new THREE.Matrix4()
const _frameTmp = new THREE.Matrix4()

/** Add side pillars, skipping corners where a corner pillar replaces them. */
function addFramePillars(
  target: GeoEntry[],
  x: number,
  yBase: number,
  z: number,
  rotY: number,
  segW: number,
  wh: number,
  isNS: boolean,
  skipLeft: boolean,
  skipRight: boolean,
  texIdx: number
) {
  const sideW = segW * FRAME_SIDE_FRAC
  for (const sign of [-1, 1]) {
    if (sign === -1 && skipLeft) continue
    if (sign === 1 && skipRight) continue
    const offset = sign * (segW / 2 - sideW / 2)
    target.push({
      geo: bakedGeo(
        new THREE.BoxGeometry(sideW, wh, FRAME_DEPTH),
        isNS ? x + offset : x,
        yBase + wh / 2,
        !isNS ? z + offset : z,
        rotY,
        sideW,
        wh
      ),
      decor: true,
      textureIndex: texIdx,
    })
  }
}

/** Add X diagonals spanning an area of width×height. */
function addFrameXDiagonals(
  target: GeoEntry[],
  x: number,
  yBase: number,
  z: number,
  rotY: number,
  innerW: number,
  diagBottom: number,
  diagTop: number,
  texIdx: number,
  flips: readonly number[] = [-1, 1]
) {
  const diagH = diagTop - diagBottom
  if (diagH <= 0 || innerW <= 0) return

  const diagLen = Math.sqrt(innerW * innerW + diagH * diagH)
  const diagAngle = Math.atan2(diagH, innerW)
  const diagCenterY = yBase + diagBottom + diagH / 2

  for (const flipSign of flips) {
    const geo = new THREE.BoxGeometry(
      diagLen,
      FRAME_DIAG_THICKNESS,
      FRAME_DEPTH
    )

    // Combine rotZ (diagonal tilt) + rotY + translate into one matrix
    _frameMat.makeRotationZ(flipSign * diagAngle)
    if (rotY !== 0) {
      _frameTmp.makeRotationY(rotY)
      _frameTmp.setPosition(x, diagCenterY, z)
      _frameMat.premultiply(_frameTmp)
    } else {
      _frameMat.setPosition(x, diagCenterY, z)
    }
    geo.applyMatrix4(_frameMat)

    const uv = geo.getAttribute('uv')
    if (uv) {
      for (let j = 0; j < uv.count; j++) {
        uv.setXY(j, uv.getX(j) * diagLen, uv.getY(j) * FRAME_DIAG_THICKNESS)
      }
    }

    target.push({ geo, textureIndex: texIdx, decor: true })
  }
}

/** Add half-timber frame for a window wall. */
function addWindowFrameGeometry(
  target: GeoEntry[],
  x: number,
  yBase: number,
  z: number,
  rotY: number,
  segW: number,
  wh: number,
  openH: number,
  openBot: number,
  isNS: boolean,
  skipLeft: boolean,
  skipRight: boolean,
  woodTexIdx: number
) {
  const sideW = segW * FRAME_SIDE_FRAC
  const bottomH = wh * FRAME_BOTTOM_FRAC
  const innerW = segW - sideW * 2
  const beamH = wh * FRAME_BEAM_FRAC
  const beamY = wh * FRAME_BEAM_Y_FRAC

  addFramePillars(
    target,
    x,
    yBase,
    z,
    rotY,
    segW,
    wh,
    isNS,
    skipLeft,
    skipRight,
    woodTexIdx
  )

  // Bottom strip
  target.push({
    geo: bakedGeo(
      new THREE.BoxGeometry(segW, bottomH, FRAME_DEPTH),
      x,
      yBase + bottomH / 2,
      z,
      rotY,
      segW,
      bottomH
    ),
    decor: true,
    textureIndex: woodTexIdx,
  })

  // Window header
  target.push({
    geo: bakedGeo(
      new THREE.BoxGeometry(innerW, FRAME_DIAG_THICKNESS, FRAME_DEPTH),
      x,
      yBase + openBot + openH,
      z,
      rotY,
      innerW,
      FRAME_DIAG_THICKNESS
    ),
    decor: true,
    textureIndex: woodTexIdx,
  })

  // Horizontal beam above X area
  target.push({
    geo: bakedGeo(
      new THREE.BoxGeometry(innerW, beamH, FRAME_DEPTH),
      x,
      yBase + beamY,
      z,
      rotY,
      innerW,
      beamH
    ),
    decor: true,
    textureIndex: woodTexIdx,
  })

  addFrameXDiagonals(
    target,
    x,
    yBase,
    z,
    rotY,
    innerW,
    bottomH,
    beamY - beamH / 2,
    woodTexIdx
  )
}

/** Frame a window set in a stone wall: sill rail, lintel and jambs only. */
function addWindowSurround(
  target: GeoEntry[],
  x: number,
  yBase: number,
  z: number,
  rotY: number,
  wh: number,
  isNS: boolean,
  texIdx: number
) {
  const railH = wh * FRAME_BEAM_FRAC
  const outerW = WINDOW_WIDTH + SURROUND_JAMB * 2
  const beam = (w: number, h: number, along: number, y: number) =>
    target.push({
      geo: bakedGeo(
        new THREE.BoxGeometry(w, h, FRAME_DEPTH),
        x + (isNS ? along : 0),
        yBase + y,
        z + (isNS ? 0 : along),
        rotY,
        w,
        h
      ),
      decor: true,
      textureIndex: texIdx,
    })
  beam(outerW, railH, 0, wh * FRAME_BEAM_Y_FRAC)
  beam(outerW, SURROUND_JAMB, 0, WINDOW_BOTTOM + WINDOW_HEIGHT)
  for (const side of [-1, 1])
    beam(
      SURROUND_JAMB,
      WINDOW_HEIGHT,
      side * (WINDOW_WIDTH / 2 + SURROUND_JAMB / 2),
      WINDOW_BOTTOM + WINDOW_HEIGHT / 2
    )
}

/** Add half-timber frame for a solid wall. */
function addFrameGeometry(
  target: GeoEntry[],
  x: number,
  yBase: number,
  z: number,
  rotY: number,
  segW: number,
  wh: number,
  isNS: boolean,
  skipLeft: boolean,
  skipRight: boolean,
  woodTexIdx: number,
  brace: FrameBrace = 'x'
) {
  const beamH = wh * FRAME_BEAM_FRAC
  const beamY = wh * FRAME_BEAM_Y_FRAC
  const sideW = segW * FRAME_SIDE_FRAC
  const innerW = segW - sideW * 2
  const bottomH = wh * FRAME_BOTTOM_FRAC

  // Horizontal beam
  target.push({
    geo: bakedGeo(
      new THREE.BoxGeometry(segW, beamH, FRAME_DEPTH),
      x,
      yBase + beamY,
      z,
      rotY,
      segW,
      beamH
    ),
    decor: true,
    textureIndex: woodTexIdx,
  })

  addFramePillars(
    target,
    x,
    yBase,
    z,
    rotY,
    segW,
    wh,
    isNS,
    skipLeft,
    skipRight,
    woodTexIdx
  )

  // Bottom strip
  target.push({
    geo: bakedGeo(
      new THREE.BoxGeometry(segW, bottomH, FRAME_DEPTH),
      x,
      yBase + bottomH / 2,
      z,
      rotY,
      segW,
      bottomH
    ),
    decor: true,
    textureIndex: woodTexIdx,
  })

  if (brace === 'none') return
  addFrameXDiagonals(
    target,
    x,
    yBase,
    z,
    rotY,
    innerW,
    bottomH,
    brace === 'x' ? beamY - beamH / 2 : wh - TOP_PLATE_HEIGHT,
    woodTexIdx,
    brace === 'x' ? undefined : [brace]
  )
}

/** Render 1m wall segments along a wall direction. */
export function collectWallSegments(
  segments: WallConfig[],
  dir: WallDirection,
  room: RoomData,
  roomIndex: number,
  entries: FloorEntries,
  allRooms: RoomData[],
  style: ResolvedHouseStyle | null = null
) {
  const dirInfo = WALL_DIR_INFO[dir]
  const { doors } = entries
  const outerTarget = dirInfo.isFront ? entries.front : entries.back
  const line = wallLineCoord(room, dir)
  const wh = room.wallHeight
  const yBase = floorYBase(room.floorLevel, wh) + FLOOR_THICKNESS / 2
  const { localX, localZ, sizeX, sizeZ } = room
  const oh = floorOverhang(room.floorLevel)

  // Wall span: shrink by WALL_THICKNESS to avoid overlap at corners
  const halfT = WALL_THICKNESS / 2
  const numSegs = segments.length
  const wallSpan = (dirInfo.isNS ? sizeX : sizeZ) + oh * 2 - WALL_THICKNESS
  const segW = numSegs > 0 ? wallSpan / numSegs : 1
  const along0 = dirInfo.isNS ? localX : localZ
  const frameTex = style ? STYLE_TEX.frame : DOOR_TEXTURE_IDX
  const ground = room.floorLevel === 0
  const outX = dir === 'east' ? 1 : dir === 'west' ? -1 : 0
  const outZ = dir === 'south' ? 1 : dir === 'north' ? -1 : 0

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i]
    if (seg.variant === 'open') continue

    // Double doors: the leader renders both segments; a lone half paired across
    // the room boundary opens through that side, otherwise it is a plain door
    let variant = seg.variant
    let span = 1
    let boundary: -1 | 0 | 1 = 0
    if (variant === 'double-door') {
      const partner = doubleDoorPartner(segments, i)
      if (partner >= 0 && partner === i - 1) continue
      if (partner >= 0) span = 2
      else {
        variant = 'door'
        if (crossRoomDoorPartner(allRooms, roomIndex, dir, i))
          boundary = i === 0 ? -1 : 1
      }
    }
    const W = segW * span
    const isDoor = isDoorVariant(variant)
    const lastIdx = i + span - 1

    const texIdx = seg.texture % HOUSING_TEXTURES.length

    // Position: offset by halfT to center within the shortened span. An
    // interior wall sits on the shared line, not pushed out by the jetty.
    const segCenter = i * segW + W / 2
    const interior = facingSegmentRef(allRooms, roomIndex, dir, i)
      ? interiorWall(
          entries,
          dirInfo.isNS,
          line,
          wh,
          along0 + i,
          along0 + i + span
        )
      : undefined
    const target = interior ? interior.entries : outerTarget
    const ohOut = interior ? 0 : oh
    let x: number, z: number, rotY: number

    switch (dir) {
      case 'north': {
        x = localX - oh + halfT + segCenter
        z = localZ - ohOut + halfT
        rotY = 0
        break
      }
      case 'south': {
        x = localX - oh + halfT + segCenter
        z = localZ + sizeZ + ohOut - halfT
        rotY = 0
        break
      }
      case 'east': {
        x = localX + sizeX + ohOut - halfT
        z = localZ - oh + halfT + segCenter
        rotY = Math.PI / 2
        break
      }
      case 'west': {
        x = localX - ohOut + halfT
        z = localZ - oh + halfT + segCenter
        rotY = Math.PI / 2
        break
      }
    }

    // fitSegment: normalize UV to 0→1 per segment instead of world-space tiling
    const fit = HOUSING_TEXTURES[texIdx].fitSegment
    const ext = style && fit && !interior ? style : null
    const stoneWall = !!ext && ext.stoneGround && ground
    const plaster = !!style && !!fit && !stoneWall
    const wallTex = stoneWall
      ? STYLE_TEX.stone
      : plaster
        ? STYLE_TEX.plaster
        : texIdx
    const wallTint = plaster ? style.palette.plaster : undefined
    const stretch = fit && wallTex === texIdx
    const hash = ext ? pieceHash(ext.seed, roomIndex, i, dir.charCodeAt(0)) : 0
    const u = (v: number) => (stretch ? v / W : v)
    const v = (h: number) => (stretch ? h / wh : h)

    if (variant === 'solid') {
      const wallH = fit ? wh - 0.01 : wh
      target.push({
        geo: bakedGeo(
          new THREE.BoxGeometry(segW, wallH, WALL_THICKNESS),
          x,
          yBase + wallH / 2,
          z,
          rotY,
          u(segW),
          v(wallH),
          plaster || stoneWall ? i * segW : 0
        ),
        textureIndex: wallTex,
        tint: wallTint,
      })
      if (fit && !stoneWall && DOOR_TEXTURE_IDX >= 0) {
        const skipL = i === 0
        const skipR = i === segments.length - 1
        addFrameGeometry(
          target,
          x,
          yBase,
          z,
          rotY,
          segW,
          wh,
          dirInfo.isNS,
          skipL,
          skipR,
          frameTex,
          !style || numSegs < 3
            ? 'x'
            : skipL || skipR
              ? (((skipL ? -1 : 1) * (dirInfo.isNS ? 1 : -1)) as -1 | 1)
              : 'none'
        )
      }
    } else {
      // Opening = segment span minus a jamb per side; a boundary side has a
      // negative jamb so the opening runs through the shared wall end
      const sideW = isDoor ? (segW - DOOR_WIDTH) / 2 : (W - WINDOW_WIDTH) / 2
      const jambL = boundary === -1 ? -halfT : sideW
      const jambR = boundary === 1 ? -halfT : sideW
      const openW = W - jambL - jambR
      const openOff = (jambL - jambR) / 2
      const openH = isDoor ? DOOR_HEIGHT : WINDOW_HEIGHT
      const openBot = isDoor ? 0 : WINDOW_BOTTOM
      const ox = dirInfo.isNS ? x + openOff : x
      const oz = dirInfo.isNS ? z : z + openOff

      // Left and right solid strips (skip for fitSegment — frame pillars cover them)
      if (sideW > 0.01 && (!fit || (stoneWall && !isDoor))) {
        for (const sign of [-1, 1]) {
          if (sign === boundary) continue
          const offset = sign * (W / 2 - sideW / 2)
          const sx = dirInfo.isNS ? x + offset : x
          const sz = !dirInfo.isNS ? z + offset : z
          const uOffX = sign === -1 ? 0 : W - sideW
          target.push({
            geo: bakedGeo(
              new THREE.BoxGeometry(sideW, wh, WALL_THICKNESS),
              sx,
              yBase + wh / 2,
              sz,
              rotY,
              u(sideW),
              v(wh),
              u(uOffX),
              0
            ),
            textureIndex: wallTex,
            tint: wallTint,
          })
        }
      }

      // Bottom strip (windows)
      if (openBot > 0.01) {
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(openW, openBot, WALL_THICKNESS),
            ox,
            yBase + openBot / 2,
            oz,
            rotY,
            u(openW),
            v(openBot),
            u(sideW),
            0
          ),
          textureIndex: wallTex,
          tint: wallTint,
        })
      }

      // Top strip
      const topH = wh - openBot - openH
      if (topH > 0.01) {
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(openW, topH, WALL_THICKNESS),
            ox,
            yBase + openBot + openH + topH / 2,
            oz,
            rotY,
            u(openW),
            v(topH),
            u(sideW),
            v(openBot + openH)
          ),
          textureIndex: wallTex,
          tint: wallTint,
        })
      }

      // Window frame geometry
      if (stoneWall && variant === 'window') {
        addWindowSurround(target, x, yBase, z, rotY, wh, dirInfo.isNS, frameTex)
      } else if (fit && variant === 'window' && DOOR_TEXTURE_IDX >= 0) {
        addWindowFrameGeometry(
          target,
          x,
          yBase,
          z,
          rotY,
          segW,
          wh,
          openH,
          openBot,
          dirInfo.isNS,
          i === 0,
          i === segments.length - 1,
          frameTex
        )
      }

      // Door frame: pillars + header beam
      if (fit && isDoor && DOOR_TEXTURE_IDX >= 0) {
        const pillarW = W * FRAME_SIDE_FRAC
        const pillarL = boundary === -1 ? -halfT : pillarW
        const pillarR = boundary === 1 ? -halfT : pillarW
        const innerW = W - pillarL - pillarR
        const beamOff = (pillarL - pillarR) / 2
        addFramePillars(
          target,
          x,
          yBase,
          z,
          rotY,
          W,
          wh,
          dirInfo.isNS,
          i === 0 || boundary === -1,
          lastIdx === segments.length - 1 || boundary === 1,
          frameTex
        )
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(innerW, FRAME_DIAG_THICKNESS, FRAME_DEPTH),
            dirInfo.isNS ? x + beamOff : x,
            yBase + DOOR_HEIGHT,
            dirInfo.isNS ? z : z + beamOff,
            rotY,
            innerW,
            FRAME_DIAG_THICKNESS
          ),
          decor: true,
          textureIndex: frameTex,
        })
      }

      const win = variant === 'window' ? ext : null
      const door = isDoor && ground ? ext : null
      // Two windows in three show a room that lights up at night.
      if (win?.decor && hash % 3 !== 0) {
        const inward = halfT + 0.01
        target.push({
          geo: bakedGeo(
            new THREE.PlaneGeometry(WINDOW_WIDTH, WINDOW_HEIGHT),
            ox - outX * inward,
            yBase + WINDOW_BOTTOM + WINDOW_HEIGHT / 2,
            oz - outZ * inward,
            OUTWARD_ROT_Y[dir],
            ...DECOR_UV.glow
          ),
          textureIndex: STYLE_TEX.decor,
        })
      }
      if (door?.stoneGround) {
        const out = FRAME_DEPTH / 2 + DOOR_STEP_DEPTH / 2
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(W, DOOR_STEP_HEIGHT, DOOR_STEP_DEPTH),
            x + outX * out,
            yBase - DOOR_STEP_HEIGHT / 2 - 0.03,
            z + outZ * out,
            rotY,
            W,
            DOOR_STEP_DEPTH
          ),
          textureIndex: STYLE_TEX.stone,
        })
      }
      // Lantern above the lintel, clear of the leaves when the door is open.
      if (door?.decor) {
        const part = (
          w: number,
          h: number,
          d: number,
          out: number,
          y: number,
          tex: number,
          uv: readonly [number, number, number, number] = [1, 1, 0, 0]
        ) =>
          target.push({
            geo: bakedGeo(
              new THREE.BoxGeometry(w, h, d),
              x + outX * (FRAME_DEPTH / 2 + out),
              yBase + y,
              z + outZ * (FRAME_DEPTH / 2 + out),
              rotY,
              ...uv
            ),
            textureIndex: tex,
          })
        part(0.04, 0.04, LANTERN_REACH, LANTERN_REACH / 2, 2.62, frameTex)
        part(0.16, 0.03, 0.16, LANTERN_REACH, 2.57, frameTex)
        part(
          0.11,
          0.17,
          0.11,
          LANTERN_REACH,
          2.47,
          STYLE_TEX.decor,
          DECOR_UV.glow
        )
        part(0.13, 0.02, 0.13, LANTERN_REACH, 2.375, frameTex)
      }
      if (win?.decor) {
        const out = FRAME_DEPTH / 2 + SHUTTER_THICKNESS / 2 + 0.005
        for (const side of [-1, 1] as const) {
          // Only over plain wall, so shutters never cross a neighbouring opening.
          if (segments[i + side]?.variant !== 'solid') continue
          const along = side * (WINDOW_WIDTH / 2 + SHUTTER_WIDTH / 2 + 0.01)
          target.push({
            geo: bakedGeo(
              new THREE.BoxGeometry(
                SHUTTER_WIDTH,
                WINDOW_HEIGHT,
                SHUTTER_THICKNESS
              ),
              ox + outX * out + (dirInfo.isNS ? along : 0),
              yBase + WINDOW_BOTTOM + WINDOW_HEIGHT / 2,
              oz + outZ * out + (dirInfo.isNS ? 0 : along),
              rotY,
              ...DECOR_UV.shutter
            ),
            textureIndex: STYLE_TEX.decor,
            tint: win.palette.shutter,
          })
        }
      }
      const sillTop =
        yBase + wh * FRAME_BEAM_Y_FRAC + (wh * FRAME_BEAM_FRAC) / 2
      if (win?.decor) {
        const boxOut = FRAME_DEPTH / 2 + FLOWER_BOX_DEPTH / 2
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(
              FLOWER_BOX_WIDTH,
              FLOWER_BOX_HEIGHT,
              FLOWER_BOX_DEPTH
            ),
            ox + outX * boxOut,
            sillTop - FLOWER_BOX_HEIGHT / 2,
            oz + outZ * boxOut,
            rotY,
            FLOWER_BOX_WIDTH,
            FLOWER_BOX_HEIGHT
          ),
          textureIndex: frameTex,
        })
        const flowerOut = FRAME_DEPTH / 2 + FLOWER_BOX_DEPTH + 0.01
        target.push({
          geo: bakedGeo(
            new THREE.PlaneGeometry(WINDOW_WIDTH, FLOWER_HEIGHT),
            ox + outX * flowerOut,
            sillTop + FLOWER_HEIGHT / 2 - FLOWER_BOX_HEIGHT * 0.8,
            oz + outZ * flowerOut,
            OUTWARD_ROT_Y[dir],
            ...DECOR_UV.flowers
          ),
          textureIndex: STYLE_TEX.decor,
        })
      } else if (win) {
        const out = halfT + SILL_DEPTH / 2
        target.push({
          geo: bakedGeo(
            new THREE.BoxGeometry(SILL_WIDTH, SILL_HEIGHT, SILL_DEPTH),
            ox + outX * out,
            sillTop - SILL_HEIGHT / 2,
            oz + outZ * out,
            rotY,
            SILL_WIDTH,
            SILL_HEIGHT
          ),
          decor: true,
          textureIndex: frameTex,
        })
      }

      // Shared panel setup for door / window hinged panels
      if (isDoor || variant === 'window') {
        const panelTexIdx = style
          ? STYLE_TEX.door
          : DOOR_TEXTURE_IDX >= 0
            ? DOOR_TEXTURE_IDX
            : texIdx
        const sashTexIdx = style ? STYLE_TEX.sash : SHUTTER_PANEL_TEXTURE_IDX
        const panelMat = getHousingMaterial(panelTexIdx)
        const isOpen = seg.isOpen ?? false
        // Inset hinge to clear frame pillar
        const inset = Math.max(0, W * FRAME_SIDE_FRAC - sideW)

        if (isDoor) {
          const doorPanelH = DOOR_HEIGHT - FRAME_DIAG_THICKNESS / 2
          const leafW = span === 2 ? openW / 2 : openW
          const panelGeo = new THREE.BoxGeometry(
            leafW,
            doorPanelH,
            WALL_THICKNESS
          )
          // Interior-face offset
          const panelZ = (dirInfo.isFront ? -1 : 1) * (WALL_THICKNESS / 4)
          // E/W closes at -PI/2: panel local +X -> world +Z fills the opening
          const closedAngle = dirInfo.isNS ? 0 : -Math.PI / 2

          // Single door hinges on the low side (away from a boundary);
          // double doors add a mirrored leaf
          const hinges: (-1 | 1)[] =
            span === 2 ? [-1, 1] : boundary === -1 ? [1] : [-1]
          for (const h of hinges) {
            const segIdx = h === -1 ? i : lastIdx
            const panel = new THREE.Mesh(panelGeo, panelMat)
            panel.userData.textureIndex = panelTexIdx
            panel.position.set(-h * (leafW / 2 - inset), doorPanelH / 2, panelZ)

            const pivot = new THREE.Group()
            pivot.name = `door_r${roomIndex}_${dir}_${segIdx}`

            const hingeOffset = openOff + h * (openW / 2 - inset)
            const openAngle = closedAngle + (h * Math.PI) / 2
            if (dirInfo.isNS) {
              pivot.position.set(x + hingeOffset, yBase, z)
            } else {
              pivot.position.set(x, yBase, z + hingeOffset)
            }
            pivot.rotation.y = isOpen ? openAngle : closedAngle
            pivot.add(panel)

            doors.push({
              pivot,
              interactionPosition: { x: ox, z: oz },
              isWindow: false,
              roomIndex,
              wallDir: dir,
              segmentIndex: segIdx,
              floorLevel: room.floorLevel,
              isOpen,
              closedAngle,
              openAngle,
              interior: interior?.wall,
            })
          }
        } else {
          // Two hinged shutters per window: single box with composite texture
          // Interior-face Z offset
          const panelZ =
            (dirInfo.isFront === dirInfo.isNS ? -1 : 1) * (WALL_THICKNESS / 4)
          const closedAngle = dirInfo.isNS ? 0 : Math.PI / 2
          const halfW = WINDOW_WIDTH / 2
          const outwardSign = dirInfo.isFront ? 1 : -1

          // Panel height trimmed to fit between frame beam top and header bottom
          const beamTop = wh * FRAME_BEAM_Y_FRAC + (wh * FRAME_BEAM_FRAC) / 2
          const headerBot =
            WINDOW_BOTTOM + WINDOW_HEIGHT - FRAME_DIAG_THICKNESS / 2
          const panelH = headerBot - beamTop
          const panelYOff = beamTop - WINDOW_BOTTOM + panelH / 2

          const shutterGeo = new THREE.BoxGeometry(
            halfW,
            panelH,
            WALL_THICKNESS / 4
          )
          // Remap side face UVs to wood border region of composite texture
          {
            const uv = shutterGeo.getAttribute('uv')
            const woodU = SHUTTER_BORDER / halfW / 2
            const woodV = 0.5
            // Vertices 0–15 are the 4 side faces (+X, -X, +Y, -Y)
            for (let vi = 0; vi < 16; vi++) {
              uv.setXY(vi, woodU, woodV)
            }
          }
          const shutterMat = getHousingMaterial(sashTexIdx)
          const clickTarget = new THREE.Mesh(
            new THREE.PlaneGeometry(openW, WINDOW_HEIGHT),
            WINDOW_CLICK_MATERIAL
          )
          clickTarget.visible = false
          clickTarget.position.set(
            ox,
            yBase + WINDOW_BOTTOM + WINDOW_HEIGHT / 2,
            oz
          )
          if (!dirInfo.isNS) clickTarget.rotation.y = Math.PI / 2

          for (const side of [-1, 1] as const) {
            const panelX = (dirInfo.isNS ? -side : side) * (halfW / 2 - inset)

            const shutter = new THREE.Mesh(shutterGeo, shutterMat)
            shutter.userData.textureIndex = sashTexIdx
            shutter.position.set(panelX, panelYOff, panelZ)

            const pivot = new THREE.Group()
            pivot.name = `win_r${roomIndex}_${dir}_${i}_${side < 0 ? 'L' : 'R'}`

            const hingeOffset = side * (WINDOW_WIDTH / 2 - inset)
            const openAngle = closedAngle + outwardSign * side * (Math.PI / 2)

            if (dirInfo.isNS) {
              pivot.position.set(x + hingeOffset, yBase + WINDOW_BOTTOM, z)
            } else {
              pivot.position.set(x, yBase + WINDOW_BOTTOM, z + hingeOffset)
            }
            pivot.rotation.y = isOpen ? openAngle : closedAngle

            pivot.add(shutter)

            doors.push({
              pivot,
              clickTarget: side === -1 ? clickTarget : undefined,
              interactionPosition: { x: ox, z: oz },
              isWindow: true,
              roomIndex,
              wallDir: dir,
              segmentIndex: i,
              floorLevel: room.floorLevel,
              isOpen,
              closedAngle,
              openAngle,
              interior: interior?.wall,
            })
          }
        }
      }
    }
    if (style && fit) {
      target.push({
        geo: bakedGeo(
          new THREE.BoxGeometry(W, TOP_PLATE_HEIGHT, FRAME_DEPTH),
          x,
          yBase + wh - TOP_PLATE_HEIGHT / 2,
          z,
          rotY,
          W,
          TOP_PLATE_HEIGHT
        ),
        decor: true,
        textureIndex: frameTex,
      })
    }
    if (ext && ground && !isDoor && !stoneWall)
      addPlinth(target, segW, x, yBase, z, rotY, i * segW)
    if (
      ext?.decor &&
      ext.ivy &&
      ground &&
      variant === 'solid' &&
      numSegs >= 3 &&
      (i === 0 || i === numSegs - 1) &&
      hash % 3 === 0
    ) {
      const out = FRAME_DEPTH / 2 + 0.015
      target.push({
        geo: bakedGeo(
          new THREE.PlaneGeometry(IVY_WIDTH, IVY_HEIGHT),
          x + outX * out,
          yBase + PLINTH_HEIGHT - 0.15 + IVY_HEIGHT / 2,
          z + outZ * out,
          OUTWARD_ROT_Y[dir],
          ...DECOR_UV.ivy
        ),
        textureIndex: STYLE_TEX.decor,
      })
    }
    if (ext && !ground) addJettyBeam(target, W, x, yBase, z, rotY)
  }

  // Corner pillars: centered at wall center-line intersection, sized to FRAME_DEPTH
  // Only NS walls draw them to avoid doubles
  if (DOOR_TEXTURE_IDX >= 0 && numSegs > 0 && dirInfo.isNS) {
    const firstFit =
      HOUSING_TEXTURES[segments[0].texture % HOUSING_TEXTURES.length].fitSegment
    const lastFit =
      HOUSING_TEXTURES[segments[numSegs - 1].texture % HOUSING_TEXTURES.length]
        .fitSegment
    const cornerSize = FRAME_DEPTH
    const stoneCorner = !!style && style.stoneGround && ground

    for (const end of [0, 1] as const) {
      const isFit = end === 0 ? firstFit : lastFit
      if (!isFit) continue
      // No pillar where a boundary half-door meets the corner
      const endSeg = end === 0 ? 0 : numSegs - 1
      const perpDir: WallDirection = end === 0 ? 'west' : 'east'
      const perpSeg =
        dir === 'north' ? 0 : getWallByDir(room, perpDir).length - 1
      if (
        crossRoomDoorPartner(allRooms, roomIndex, dir, endSeg) ||
        crossRoomDoorPartner(allRooms, roomIndex, perpDir, perpSeg)
      )
        continue

      // Place at the intersection of this wall's center and the perpendicular wall's center
      const sign = end === 0 ? -1 : 1
      const alongOffset = (sign * wallSpan) / 2

      const cx = localX - oh + halfT + wallSpan / 2 + alongOffset
      const cz =
        dir === 'north' ? localZ - oh + halfT : localZ + sizeZ + oh - halfT

      outerTarget.push({
        geo: bakedGeo(
          new THREE.BoxGeometry(cornerSize, wh, cornerSize),
          cx,
          yBase + wh / 2,
          cz,
          0,
          cornerSize,
          wh
        ),
        textureIndex: stoneCorner ? STYLE_TEX.stone : frameTex,
      })
      if (style && ground && !stoneCorner)
        addPlinth(outerTarget, PLINTH_DEPTH, cx, yBase, cz, 0)
      if (style && !ground)
        addJettyBeam(outerTarget, JETTY_BEAM_DEPTH, cx, yBase, cz, 0)
    }
  }
}
