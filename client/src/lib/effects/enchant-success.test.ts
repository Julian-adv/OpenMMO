import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import {
  EnchantSuccessEffect,
  ENCHANT_SUCCESS_DURATION,
} from './enchant-success'
import { ENCHANT_LIGHT_INTENSITY } from '../utils/enchantLight'

describe('Enchant success', () => {
  it('follows the hand, gathers light and fades completely after release', () => {
    const effect = new EnchantSuccessEffect(new THREE.Texture())
    const camera = new THREE.PerspectiveCamera()
    const hand = new THREE.Vector3(3, 2, 1)
    const anchor = { position: hand, weapon: null }
    effect.update(0.06, anchor, camera)
    expect(effect.group.position.toArray()).toEqual([3, 2, 1.5])
    const earlyIntensity = effect.light.intensity
    hand.x = 4
    effect.update(0.27, anchor, camera)
    expect(effect.group.position.x).toBe(4)
    expect(effect.light.position.equals(effect.group.position)).toBe(true)
    expect(
      effect.group.children.some((object) => object instanceof THREE.PointLight)
    ).toBe(false)
    expect(effect.light.intensity).toBeGreaterThan(earlyIntensity)
    effect.update(ENCHANT_SUCCESS_DURATION, anchor, camera)
    expect(effect.group.visible).toBe(false)
    expect(effect.light.intensity).toBe(0)
    effect.dispose()
    expect(effect.group.children).toHaveLength(0)
  })

  it('clears the opaque torso while preserving screen alignment and world occlusion', () => {
    const effect = new EnchantSuccessEffect(new THREE.Texture())
    const torso = new THREE.Box3(
      new THREE.Vector3(-0.3, 0.6, -0.3),
      new THREE.Vector3(0.3, 1.6, 0.3)
    )
    const anchor = torso.getCenter(new THREE.Vector3())
    const camera = new THREE.OrthographicCamera(-5, 5, 5, -5, 0.1, 100)
    for (const position of [
      [0, 5, 10],
      [10, 5, 0],
      [0, 5, -10],
    ]) {
      camera.position.fromArray(position)
      camera.lookAt(anchor)
      camera.updateMatrixWorld(true)
      effect.update(0.3, { position: anchor, weapon: null }, camera)
      expect(torso.containsPoint(effect.group.position)).toBe(false)
      const projectedAnchor = anchor.clone().project(camera)
      const projectedEffect = effect.group.position.clone().project(camera)
      expect(projectedEffect.x).toBeCloseTo(projectedAnchor.x)
      expect(projectedEffect.y).toBeCloseTo(projectedAnchor.y)
      expect(projectedEffect.z).toBeLessThan(projectedAnchor.z)
    }
    for (const child of effect.group.children) {
      const material = (child as THREE.Mesh).material as THREE.Material
      expect(material.depthTest).toBe(true)
    }
    effect.dispose()
  })

  it('emits every ray from one point on the blade axis and follows its pose', () => {
    const weapon = new THREE.Mesh(new THREE.BoxGeometry(0.15, 2, 0.08))
    const effect = new EnchantSuccessEffect(new THREE.Texture())
    const camera = new THREE.PerspectiveCamera()
    const anchor = { position: new THREE.Vector3(), weapon }
    const ray = new THREE.Matrix4()
    const origin = new THREE.Vector3()
    const localBounds = new THREE.Box3(
      new THREE.Vector3(-0.075, -1, -0.04),
      new THREE.Vector3(0.075, 1, 0.04)
    ).expandByScalar(1e-5)
    let previous: number[] | undefined
    for (const progress of [0.2, 0.6]) {
      const elapsed = ENCHANT_SUCCESS_DURATION * progress
      weapon.position.set(elapsed, 2, -3)
      weapon.rotation.z = elapsed * 0.2
      effect.update(elapsed, anchor, camera)
      const rays = effect.group.getObjectByName(
        'enchant-weapon-rays'
      ) as THREE.InstancedMesh
      expect(rays.count).toBeGreaterThan(0)
      expect(rays.count).toBeLessThanOrEqual(16)
      let firstOrigin: THREE.Vector3 | undefined
      for (let i = 0; i < rays.count; i++) {
        rays.getMatrixAt(i, ray)
        origin.setFromMatrixPosition(ray).add(effect.group.position)
        origin.z -= 0.12
        weapon.worldToLocal(origin)
        expect(localBounds.containsPoint(origin)).toBe(true)
        expect(origin.x).toBeCloseTo(0)
        expect(origin.z).toBeCloseTo(0)
        const rayDirection = new THREE.Vector3()
          .setFromMatrixColumn(ray, 1)
          .normalize()
        expect(rayDirection.length()).toBeCloseTo(1)
        expect(rayDirection.z).toBeCloseTo(0)
        if (firstOrigin)
          expect(origin.distanceTo(firstOrigin)).toBeLessThan(1e-6)
        else firstOrigin = origin.clone()
      }
      const opacity = Array.from(
        rays.geometry.getAttribute('aEnchantRayOpacity').array
      )
      if (previous) expect(opacity).not.toEqual(previous)
      previous = opacity
      expect(effect.light.intensity).toBe(ENCHANT_LIGHT_INTENSITY)
    }
    effect.update(ENCHANT_SUCCESS_DURATION * 0.7, anchor, camera, true)
    expect(
      (
        effect.group.getObjectByName(
          'enchant-weapon-rays'
        ) as THREE.InstancedMesh
      ).count
    ).toBeLessThanOrEqual(8)
    effect.update(
      ENCHANT_SUCCESS_DURATION * 0.8,
      { position: anchor.position, weapon: null },
      camera
    )
    expect(effect.group.getObjectByName('enchant-weapon-rays')!.visible).toBe(
      false
    )
    expect(effect.group.visible).toBe(true)
    effect.update(ENCHANT_SUCCESS_DURATION, anchor, camera)
    expect(effect.group.visible).toBe(false)
    effect.dispose()
    weapon.geometry.dispose()
  })

  it('limits simultaneous rays while staggering their emission over time', () => {
    const weapon = new THREE.Mesh(new THREE.BoxGeometry(0.15, 2, 0.08))
    const effect = new EnchantSuccessEffect(new THREE.Texture())
    const camera = new THREE.PerspectiveCamera()
    const anchor = { position: new THREE.Vector3(), weapon }
    const rays = effect.group.getObjectByName(
      'enchant-weapon-rays'
    ) as THREE.InstancedMesh
    const matrix = new THREE.Matrix4()
    const directions = new Set<string>()
    for (const reduced of [false, true]) {
      for (let frame = 0; frame < 100; frame++) {
        effect.update(0.3 + frame * 0.031, anchor, camera, reduced)
        expect(rays.count).toBeGreaterThan(0)
        expect(rays.count).toBeLessThanOrEqual(reduced ? 8 : 16)
        for (let i = 0; i < rays.count; i++) {
          const opacity = rays.geometry
            .getAttribute('aEnchantRayOpacity')
            .getX(i)
          expect(opacity).toBeGreaterThan(0)
          expect(opacity).toBeLessThanOrEqual(1)
          rays.getMatrixAt(i, matrix)
          const direction = new THREE.Vector3()
            .setFromMatrixColumn(matrix, 1)
            .normalize()
          directions.add(
            direction
              .toArray()
              .map((value) => value.toFixed(2))
              .join(',')
          )
        }
      }
    }
    expect(directions.size).toBeGreaterThanOrEqual(8)
    effect.dispose()
    weapon.geometry.dispose()
  })
})
