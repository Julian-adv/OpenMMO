import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { TeleportEffectSystem } from './teleport'
import {
  TELEPORT_ARRIVAL_MS,
  TELEPORT_REVEAL_MS,
  TELEPORT_VANISH_MS,
} from '../stores/teleportEffectStore'

describe('teleport beam', () => {
  it('rises from the ground on departure and descends to the reveal point on arrival', () => {
    const effect = new TeleportEffectSystem()
    const camera = new THREE.PerspectiveCamera()
    const beam = effect.group.children[0]
    effect.update(50, 'Departing', camera)
    const earlyTop = beam.position.y + beam.scale.y / 2
    expect(beam.position.y - beam.scale.y / 2).toBeCloseTo(0)
    effect.update(TELEPORT_VANISH_MS, 'Departing', camera)
    expect(beam.position.y + beam.scale.y / 2).toBeGreaterThan(earlyTop)
    effect.update(50, 'Arriving', camera)
    expect(beam.position.y - beam.scale.y / 2).toBeGreaterThan(1.8)
    effect.update(TELEPORT_REVEAL_MS, 'Arriving', camera)
    expect(beam.position.y - beam.scale.y / 2).toBeCloseTo(0)
    expect(effect.update(TELEPORT_ARRIVAL_MS, 'Arriving', camera)).toBe(false)
    expect(effect.group.visible).toBe(false)
    effect.dispose()
  })
})
