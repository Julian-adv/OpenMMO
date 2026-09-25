import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import {
  EnchantSuccessEffect,
  ENCHANT_SUCCESS_DURATION,
} from './enchant-success'
import {
  ENCHANT_LIGHT_INTENSITY,
  ENCHANT_ARMOR_LIGHT_INTENSITY,
} from '../utils/enchantLight'

describe('Enchant success', () => {
  it('follows the torso, gathers light and fades completely after release', () => {
    const effect = new EnchantSuccessEffect()
    const camera = new THREE.PerspectiveCamera()
    const torso = new THREE.Vector3(3, 2, 1)
    const anchor = { position: torso, weapon: null }
    effect.update(0.06, anchor, camera)
    expect(effect.group.position.toArray()).toEqual([3, 2, 1.5])
    const earlyIntensity = effect.light.intensity
    torso.x = 4
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
    const effect = new EnchantSuccessEffect()
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
    const effect = new EnchantSuccessEffect()
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
        'enchant-rays'
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
      (effect.group.getObjectByName('enchant-rays') as THREE.InstancedMesh)
        .count
    ).toBeLessThanOrEqual(8)
    effect.update(
      ENCHANT_SUCCESS_DURATION * 0.8,
      { position: anchor.position, weapon: null },
      camera
    )
    expect(effect.group.getObjectByName('enchant-rays')!.visible).toBe(true)
    expect(
      effect.group.getObjectByName('enchant-weapon-blade-glow')!.visible
    ).toBe(false)
    expect(
      effect.group.getObjectByName('enchant-weapon-axis-glow')!.visible
    ).toBe(false)
    expect(effect.group.getObjectByName('enchant-armor-glow')!.visible).toBe(
      true
    )
    expect(effect.group.visible).toBe(true)
    effect.update(ENCHANT_SUCCESS_DURATION, anchor, camera)
    expect(effect.group.visible).toBe(false)
    effect.dispose()
    weapon.geometry.dispose()
  })

  it('limits simultaneous rays while staggering their emission over time', () => {
    const weapon = new THREE.Mesh(new THREE.BoxGeometry(0.15, 2, 0.08))
    const effect = new EnchantSuccessEffect()
    const camera = new THREE.PerspectiveCamera()
    const anchor = { position: new THREE.Vector3(), weapon }
    const rays = effect.group.getObjectByName(
      'enchant-rays'
    ) as THREE.InstancedMesh
    const matrix = new THREE.Matrix4()
    const directions = new Set<string>()
    for (const reduced of [false, true]) {
      for (let frame = 0; frame < 100; frame++) {
        effect.update(0.35 + frame * 0.031, anchor, camera, reduced)
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

  it('radiates every armor ray from one chest point while the torso and camera move', () => {
    const effect = new EnchantSuccessEffect()
    const camera = new THREE.PerspectiveCamera()
    const anchor = { position: new THREE.Vector3(3, 1.2, -2), weapon: null }
    const rays = effect.group.getObjectByName(
      'enchant-rays'
    ) as THREE.InstancedMesh
    const matrix = new THREE.Matrix4()
    const cameraDirection = new THREE.Vector3()
    const origin = new THREE.Vector3()
    const direction = new THREE.Vector3()
    for (const reduced of [false, true]) {
      for (let frame = 0; frame < 100; frame++) {
        anchor.position.x += 0.01
        camera.position.set(
          Math.sin(frame * 0.1) * 8,
          6,
          Math.cos(frame * 0.1) * 8
        )
        camera.lookAt(anchor.position)
        camera.updateMatrixWorld(true)
        camera.getWorldDirection(cameraDirection)
        effect.update(0.35 + frame * 0.031, anchor, camera, reduced)
        expect(rays.count).toBeGreaterThan(0)
        expect(rays.count).toBeLessThanOrEqual(reduced ? 10 : 20)
        for (let i = 0; i < rays.count; i++) {
          rays.getMatrixAt(i, matrix)
          origin
            .setFromMatrixPosition(matrix)
            .add(effect.group.position)
            .addScaledVector(cameraDirection, 0.5)
          expect(origin.distanceTo(anchor.position)).toBeLessThan(1e-6)
          direction.setFromMatrixColumn(matrix, 1)
          expect(direction.length()).toBeLessThanOrEqual(1.5 + 1e-6)
          direction.normalize()
          expect(direction.dot(cameraDirection)).toBeCloseTo(0)
          const opacity = rays.geometry
            .getAttribute('aEnchantRayOpacity')
            .getX(i)
          expect(opacity).toBeGreaterThan(0)
          expect(opacity).toBeLessThanOrEqual(1)
        }
      }
    }
    effect.dispose()
  })

  it('keeps armor particles moving during the hold and resets pooled weapon effects', () => {
    const effect = new EnchantSuccessEffect()
    const camera = new THREE.PerspectiveCamera()
    const anchor = { position: new THREE.Vector3(0, 1.2, 0), weapon: null }
    const dust = effect.group.getObjectByName(
      'enchant-motes'
    ) as THREE.InstancedMesh
    const weapon = new THREE.Mesh(new THREE.BoxGeometry(0.15, 2, 0.08))
    effect.update(1, { ...anchor, weapon }, camera)
    expect(effect.group.getObjectByName('enchant-armor-glow')!.visible).toBe(
      false
    )
    effect.clear()
    effect.update(1, anchor, camera)
    expect(effect.group.getObjectByName('enchant-armor-glow')!.visible).toBe(
      true
    )
    expect(
      effect.group.getObjectByName('enchant-weapon-blade-glow')!.visible
    ).toBe(false)
    expect(dust.count).toBe(32)
    const particles = Array.from(dust.instanceMatrix.array)
    effect.update(2, anchor, camera)
    expect(Array.from(dust.instanceMatrix.array)).not.toEqual(particles)
    effect.update(3, anchor, camera, true)
    expect(dust.count).toBe(8)
    expect(effect.light.intensity).toBe(ENCHANT_ARMOR_LIGHT_INTENSITY)
    effect.update(4.85, anchor, camera)
    expect(effect.light.intensity).toBeCloseTo(
      ENCHANT_ARMOR_LIGHT_INTENSITY / 2
    )
    effect.update(ENCHANT_SUCCESS_DURATION, anchor, camera)
    expect(effect.group.visible).toBe(false)
    expect(effect.light.intensity).toBe(0)
    effect.dispose()
    weapon.geometry.dispose()
  })

  it('builds long armor rays from the torso before sending their light outward', () => {
    const effect = new EnchantSuccessEffect()
    const camera = new THREE.PerspectiveCamera()
    const anchor = { position: new THREE.Vector3(0, 1.2, 0), weapon: null }
    const rays = effect.group.getObjectByName(
      'enchant-rays'
    ) as THREE.InstancedMesh
    const motes = effect.group.getObjectByName(
      'enchant-motes'
    ) as THREE.InstancedMesh
    const matrix = new THREE.Matrix4()
    const point = new THREE.Vector3()
    effect.update(0.29, anchor, camera)
    expect(rays.count).toBe(0)
    effect.update(0.4, anchor, camera)
    rays.getMatrixAt(0, matrix)
    const earlyLength = point.setFromMatrixColumn(matrix, 1).length()
    const earlyProgress = rays.geometry
      .getAttribute('aEnchantRayProgress')
      .getX(0)
    motes.getMatrixAt(0, matrix)
    const earlyMoteRadius = point.setFromMatrixPosition(matrix).length()
    effect.update(0.65, anchor, camera)
    rays.getMatrixAt(0, matrix)
    expect(point.setFromMatrixColumn(matrix, 1).length()).toBeGreaterThan(
      earlyLength * 3
    )
    expect(
      rays.geometry.getAttribute('aEnchantRayProgress').getX(0)
    ).toBeGreaterThan(earlyProgress)
    motes.getMatrixAt(0, matrix)
    expect(point.setFromMatrixPosition(matrix).length()).toBeGreaterThan(
      earlyMoteRadius
    )
    effect.dispose()
  })
})
