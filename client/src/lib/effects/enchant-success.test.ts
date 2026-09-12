import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { EnchantSuccessEffect } from './enchant-success'

describe('Enchant success', () => {
  it('follows the hand, gathers light and fades completely after release', () => {
    const effect = new EnchantSuccessEffect(new THREE.Texture())
    const camera = new THREE.PerspectiveCamera()
    const hand = new THREE.Vector3(3, 2, 1)
    effect.update(0.2, null, hand, camera)
    expect(effect.group.position.equals(hand)).toBe(true)
    const earlyIntensity = effect.light.intensity
    hand.x = 4
    effect.update(0.9, null, hand, camera)
    expect(effect.group.position.x).toBe(4)
    expect(effect.light.position.equals(hand)).toBe(true)
    expect(effect.group.children).not.toContain(effect.light)
    expect(effect.light.intensity).toBeGreaterThan(earlyIntensity)
    effect.update(1, 0.6, hand, camera)
    expect(effect.group.visible).toBe(false)
    expect(effect.light.intensity).toBe(0)
    effect.dispose()
    expect(effect.group.children).toHaveLength(0)
  })
})
