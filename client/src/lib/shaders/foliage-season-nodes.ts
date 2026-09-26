import type { Node } from 'three/webgpu'
import {
  dot,
  mix,
  mx_noise_float,
  positionWorld,
  smoothstep,
  step,
  uniform,
  varying,
  vec3,
  vec4,
} from 'three/tsl'
import { foliageSeason } from '../utils/foliageSeason'
import { perlin } from './snow-cover-nodes'

export const grassDry = uniform(0)
const leafTurn = uniform(0)
const leafFall = uniform(0)

let shownDay = NaN

export function setFoliageDay(day: number) {
  if (Math.abs(day - shownDay) < 0.01) return
  shownDay = day
  const s = foliageSeason(day)
  grassDry.value = s.grassDry
  leafTurn.value = s.leafTurn
  leafFall.value = s.leafFall
}

export function leavesLeft(): boolean {
  return leafFall.value < 1
}

/** Autumn colour and leaf drop for leaf cards. Whole trees and ~1 m clumps
 *  share a value so the canopy thins in patches, not speckle. */
export function leafSeason(base: Node<'vec4'>): Node<'vec4'> {
  const tree = varying(perlin(positionWorld.xz, 0.07, 0.4, 0))
  const clump = mx_noise_float(positionWorld.mul(0.9)).mul(0.5).add(0.5)
  const order = tree.mul(0.55).add(clump.mul(0.45)).clamp(0.01, 0.99)

  const hue = varying(perlin(positionWorld.xz, 0.35, 1.3, 31.7))
  const luma = dot(base.rgb, vec3(0.299, 0.587, 0.114))
  const autumn = mix(
    mix(
      vec3(0.7, 0.08, 0.02),
      vec3(1.0, 0.38, 0.03),
      smoothstep(0.2, 0.5, hue)
    ),
    vec3(1.0, 0.72, 0.1),
    smoothstep(0.5, 0.8, hue)
  ).mul(luma.mul(3.4))
  // Trees that drop early also turn early.
  const turn = smoothstep(0, 1, leafTurn.mul(1.6).sub(order.mul(0.6)))
  const kept = step(leafFall, order)
  return vec4(mix(base.rgb, autumn, turn), base.a.mul(kept))
}
