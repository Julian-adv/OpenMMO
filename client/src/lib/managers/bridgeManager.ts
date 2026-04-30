import type {
  BridgeMeta,
  ObjectDef,
  ObjectPlacement,
} from '../stores/editorStore'
import { rotatedRectAabb } from '../utils/objectFootprint'

interface RegisteredBridge {
  px: number
  py: number
  pz: number
  /** three.js Y-rotation convention (positive Y rotates +Z toward +X). */
  cosRot: number
  sinRot: number
  halfLen: number
  worldMinX: number
  worldMaxX: number
  worldMinZ: number
  worldMaxZ: number
  /** railOuterOffset resolved against the default — precomputed for hot paths. */
  pad: number
  /** worldMin/Max ± pad — padded AABB for the per-step movement-collision reject. */
  paddedMinX: number
  paddedMaxX: number
  paddedMinZ: number
  paddedMaxZ: number
  meta: BridgeMeta
}

/** Player Y must be within this distance of deckY to count as "on the deck".
 *  Set to span the full arch height (~1.5m on stone_bridge) so that a player
 *  who entered at the abutment stays snapped to deckY across the arch crown. */
const DECK_Y_TOLERANCE = 1.5

/** Minimum vertical clearance from player feet to deck Y for the bridge to be
 *  treated as "walk-under" geometry. Below this, the railing applies (player is
 *  effectively at deck level, even if their literal head fits beneath). Set
 *  generously so typical low-arch bridges (crown ~2.4m) still block side entry
 *  from the bank instead of letting a ground-level player slip through under
 *  the rail. */
const MIN_DECK_OVERHEAD_CLEARANCE = 2.5

/** Default entry-side stop offset (used when a bridge model omits
 *  `railOuterOffset`). Conservative small value — bridges with thicker
 *  parapet/abutment structures should override via meta. */
const DEFAULT_RAIL_ENTRY_OUTSIDE_OFFSET = 0.3

/** Extra slack added to the fade-skip rect on top of `railOuterOffset` to
 *  absorb per-step overshoot past the movement outer line. Independent from
 *  the offset itself — tune separately. */
const FADE_SKIP_OVERSHOOT_CUSHION = 0.3

class BridgeManager {
  private bridges = new Map<number, RegisteredBridge>()

  syncRegion(placements: ObjectPlacement[], catalog: Map<string, ObjectDef>) {
    for (const p of placements) {
      const d = catalog.get(p.type)
      if (d?.kind !== 'bridge' || !d.bridge) continue
      const rot = (p.rotation * Math.PI) / 180
      const m = d.bridge
      const halfLen =
        m.deckAxis === 'z'
          ? Math.max(Math.abs(m.deckMinZ), Math.abs(m.deckMaxZ))
          : Math.max(Math.abs(m.deckMinX), Math.abs(m.deckMaxX))
      const aabb = rotatedRectAabb(
        m.deckMinX,
        m.deckMaxX,
        m.deckMinZ,
        m.deckMaxZ,
        rot
      )
      const pad = m.railOuterOffset ?? DEFAULT_RAIL_ENTRY_OUTSIDE_OFFSET
      const worldMinX = p.x + aabb.minX
      const worldMaxX = p.x + aabb.maxX
      const worldMinZ = p.z + aabb.minZ
      const worldMaxZ = p.z + aabb.maxZ
      this.bridges.set(p.id, {
        px: p.x,
        py: p.y,
        pz: p.z,
        cosRot: Math.cos(rot),
        sinRot: Math.sin(rot),
        halfLen,
        worldMinX,
        worldMaxX,
        worldMinZ,
        worldMaxZ,
        pad,
        paddedMinX: worldMinX - pad,
        paddedMaxX: worldMaxX + pad,
        paddedMinZ: worldMinZ - pad,
        paddedMaxZ: worldMaxZ + pad,
        meta: m,
      })
    }
  }

  private toLocal(
    b: RegisteredBridge,
    wx: number,
    wz: number
  ): { lx: number; lz: number } {
    const dx = wx - b.px
    const dz = wz - b.pz
    return {
      lx: dx * b.cosRot - dz * b.sinRot,
      lz: dx * b.sinRot + dz * b.cosRot,
    }
  }

  private deckLocalY(b: RegisteredBridge, lx: number, lz: number): number {
    const m = b.meta
    const along = m.deckAxis === 'z' ? lz : lx
    if (b.halfLen <= 0) return m.deckCrownY
    const t = Math.min(1, Math.abs(along) / b.halfLen)
    return m.deckCrownY - (m.deckCrownY - m.deckEndY) * t * t
  }

  private insideRect(b: RegisteredBridge, lx: number, lz: number): boolean {
    const m = b.meta
    return (
      lx >= m.deckMinX &&
      lx <= m.deckMaxX &&
      lz >= m.deckMinZ &&
      lz <= m.deckMaxZ
    )
  }

  /** World-space AABB precheck before doing any trig — cheap reject for bridges far from (wx, wz). */
  private nearAabb(b: RegisteredBridge, wx: number, wz: number): boolean {
    return (
      wx >= b.worldMinX &&
      wx <= b.worldMaxX &&
      wz >= b.worldMinZ &&
      wz <= b.worldMaxZ
    )
  }

  private findBridgeAt(
    wx: number,
    wz: number,
    currentY: number | null
  ): {
    bridge: RegisteredBridge
    deckY: number
    lx: number
    lz: number
  } | null {
    for (const b of this.bridges.values()) {
      if (!this.nearAabb(b, wx, wz)) continue
      const { lx, lz } = this.toLocal(b, wx, wz)
      if (!this.insideRect(b, lx, lz)) continue
      const deckY = b.py + this.deckLocalY(b, lx, lz)
      if (currentY !== null && Math.abs(currentY - deckY) > DECK_Y_TOLERANCE)
        continue
      return { bridge: b, deckY, lx, lz }
    }
    return null
  }

  /** Returns deck Y at (wx, wz) if the player at currentY is on a bridge deck, else null. */
  findDeckYAt(wx: number, wz: number, currentY: number | null): number | null {
    return this.findBridgeAt(wx, wz, currentY)?.deckY ?? null
  }

  /**
   * Returns the placement id of a bridge that visually occludes the player
   * along the isometric camera ray R(s) = (px - s, py + s, pz + s), s >= 0.
   * The AABB has no lower Y bound (sLow=0) so a player directly under the
   * deck still counts as occluded — otherwise the ray would exit the XZ box
   * before climbing to the bridge bottom.
   */
  findOccludingBridgeId(px: number, py: number, pz: number): number | null {
    for (const [id, b] of this.bridges) {
      const m = b.meta
      const sHigh = b.py + m.deckCrownY - py
      if (sHigh <= 0) continue
      // Skip if the player is standing on this bridge's deck — the deck below
      // their feet doesn't occlude them from the camera, only the structure
      // *above* would, which we ignore for self-occlusion clarity.
      const { lx, lz } = this.toLocal(b, px, pz)
      const inDeckRect = this.insideRect(b, lx, lz)
      if (inDeckRect) {
        const deckY = b.py + this.deckLocalY(b, lx, lz)
        if (py >= deckY - 0.5) continue
      } else {
        // Skip fade when the player is flush against the bridge structure on
        // a long side — movement collision parks them in the parapet buffer,
        // where they sit next to the rail rather than under the deck.
        const skip = b.pad + FADE_SKIP_OVERSHOOT_CUSHION
        if (
          lx >= m.deckMinX - skip &&
          lx <= m.deckMaxX + skip &&
          lz >= m.deckMinZ - skip &&
          lz <= m.deckMaxZ + skip
        )
          continue
      }
      // Intersect the iso camera ray with the rotated deck rect in local
      // space. AABB-only checks false-positive at non-axis rotations because
      // the rotated rect's AABB is much larger than the rect itself.
      const lvx = -b.cosRot - b.sinRot
      const lvz = -b.sinRot + b.cosRot
      let sMin = 0
      let sMax = sHigh
      // Local X slab
      if (Math.abs(lvx) < 1e-9) {
        if (lx < m.deckMinX || lx > m.deckMaxX) continue
      } else {
        const s1 = (m.deckMinX - lx) / lvx
        const s2 = (m.deckMaxX - lx) / lvx
        sMin = Math.max(sMin, Math.min(s1, s2))
        sMax = Math.min(sMax, Math.max(s1, s2))
      }
      // Local Z slab
      if (Math.abs(lvz) < 1e-9) {
        if (lz < m.deckMinZ || lz > m.deckMaxZ) continue
      } else {
        const s1 = (m.deckMinZ - lz) / lvz
        const s2 = (m.deckMaxZ - lz) / lvz
        sMin = Math.max(sMin, Math.min(s1, s2))
        sMax = Math.min(sMax, Math.max(s1, s2))
      }
      if (sMin > sMax) continue
      // Ray crosses the deck rect in XZ — but it only occludes the player if
      // it enters from BELOW the deck. A player on the bank just outside the
      // deck edge still has the ray cross the rect (going up-and-into it),
      // yet the ray is already above the deck Y at the entry point, so the
      // bridge isn't actually between camera and player.
      const lxAtEntry = lx + sMin * lvx
      const lzAtEntry = lz + sMin * lvz
      const deckYAtEntry = b.py + this.deckLocalY(b, lxAtEntry, lzAtEntry)
      if (py + sMin >= deckYAtEntry) continue
      return id
    }
    return null
  }

  /** Block crossing the long-side railing of a deck. Short ends remain open for
   *  entry/exit. Symmetric: blocks both exits (from on-deck to off) and entries
   *  (from off-deck to on) so a player can't pass through the railing in either
   *  direction. A player walking under a tall bridge (head below deck Y) is not
   *  blocked. */
  isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ): boolean {
    for (const b of this.bridges.values()) {
      const fromIn =
        fromX >= b.paddedMinX &&
        fromX <= b.paddedMaxX &&
        fromZ >= b.paddedMinZ &&
        fromZ <= b.paddedMaxZ
      const toIn =
        toX >= b.paddedMinX &&
        toX <= b.paddedMaxX &&
        toZ >= b.paddedMinZ &&
        toZ <= b.paddedMaxZ
      if (!fromIn && !toIn) continue
      const { lx: fromLx, lz: fromLz } = this.toLocal(b, fromX, fromZ)
      const { lx: toLx, lz: toLz } = this.toLocal(b, toX, toZ)
      const m = b.meta
      const pad = b.pad
      let crosses = false
      if (m.deckAxis === 'z') {
        const innerMin = m.deckMinX
        const innerMax = m.deckMaxX
        const outerMin = innerMin - pad
        const outerMax = innerMax + pad
        // Exit: from on deck, to crosses the inner rect edge outward.
        if (fromLx >= innerMin && toLx < innerMin) crosses = true
        else if (fromLx <= innerMax && toLx > innerMax) crosses = true
        // Entry: from outside, to crosses the outer (parapet-clearing) line inward.
        else if (fromLx <= outerMin && toLx > outerMin) crosses = true
        else if (fromLx >= outerMax && toLx < outerMax) crosses = true
      } else {
        const innerMin = m.deckMinZ
        const innerMax = m.deckMaxZ
        const outerMin = innerMin - pad
        const outerMax = innerMax + pad
        if (fromLz >= innerMin && toLz < innerMin) crosses = true
        else if (fromLz <= innerMax && toLz > innerMax) crosses = true
        else if (fromLz <= outerMin && toLz > outerMin) crosses = true
        else if (fromLz >= outerMax && toLz < outerMax) crosses = true
      }
      if (!crosses) continue
      // Walk-under guard: if the deck overhead is high enough that the player's
      // body sits entirely below it, the railing is irrelevant.
      const refLx = m.deckAxis === 'z' ? fromLx : (fromLz + toLz) * 0.5
      const refLz = m.deckAxis === 'z' ? (fromLz + toLz) * 0.5 : fromLz
      const deckY = b.py + this.deckLocalY(b, refLx, refLz)
      if (y + MIN_DECK_OVERHEAD_CLEARANCE < deckY) continue
      return true
    }
    return false
  }
}

export const bridgeManager = new BridgeManager()
