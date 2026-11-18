import { roadSnow } from '../stores/roadSnowStore'
import {
  ROAD_CLEAR_RESOLUTION,
  ROAD_CLEAR_SIZE_M,
  roadClearActive,
  roadClearCentre,
  roadClearData,
  roadClearTexture,
} from '../shaders/snow-cover-nodes'
import { paintRoadSnowMask } from './roadSnowMask'

/** Real seconds between repaints while nothing else changed; snow creeps
 *  back over minutes. */
const REPAINT_SECONDS = 10
/** The mask is repainted once the player strays this far from its centre. */
const RECENTRE_M = ROAD_CLEAR_SIZE_M / 4

/** Keeps the cleared-road mask painted around the player. */
export class RoadSnowPainter {
  private sincePaint = Infinity
  private revision = -1
  private centreX = Infinity
  private centreZ = Infinity

  update(seconds: number, x: number, z: number, gameMinutes: number): void {
    this.sincePaint += Math.max(0, seconds)
    const moved =
      Math.abs(x - this.centreX) > RECENTRE_M ||
      Math.abs(z - this.centreZ) > RECENTRE_M
    const changed = this.revision !== roadSnow.revision
    if (!moved && !changed && this.sincePaint < REPAINT_SECONDS) return
    if (roadSnow.stretches.length === 0 && roadClearActive.value === 0) {
      this.revision = roadSnow.revision
      this.sincePaint = 0
      return
    }
    this.sincePaint = 0
    this.revision = roadSnow.revision
    if (moved) {
      this.centreX = x
      this.centreZ = z
    }
    paintRoadSnowMask(
      roadClearData,
      ROAD_CLEAR_RESOLUTION,
      ROAD_CLEAR_SIZE_M,
      this.centreX,
      this.centreZ,
      roadSnow.stretches,
      gameMinutes
    )
    roadClearCentre.value.set(this.centreX, this.centreZ)
    roadClearActive.value = roadClearData.some((value) => value > 0) ? 1 : 0
    roadClearTexture.needsUpdate = true
  }
}
