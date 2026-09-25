import * as THREE from 'three'
import { holdFirstKeyframe } from './animationTracks'

// Keep the lower spine with the seated hips to avoid bending the waist.
const SEATED_BONES = new Set([
  'Hips',
  'Spine',
  'LeftUpLeg',
  'LeftLeg',
  'LeftFoot',
  'LeftToeBase',
  'LeftToe_End',
  'RightUpLeg',
  'RightLeg',
  'RightFoot',
  'RightToeBase',
  'RightToe_End',
])

// Tracks use either `.bones[Name].prop` or `Name.prop`.
const RETARGETED = /^\.bones\[(.+?)\]\.(.+)$/

function boneOf(trackName: string): string {
  const retargeted = RETARGETED.exec(trackName)
  if (retargeted) return retargeted[1]
  const dot = trackName.lastIndexOf('.')
  return dot === -1 ? trackName : trackName.slice(0, dot)
}

const merged = new WeakMap<
  THREE.AnimationClip,
  WeakMap<THREE.AnimationClip, THREE.AnimationClip>
>()

/** Hold the seated lower body while playing the fishing clip. */
export function seatedFishingClip(
  fishing: THREE.AnimationClip,
  seated: THREE.AnimationClip
): THREE.AnimationClip {
  let bySeat = merged.get(fishing)
  if (!bySeat) {
    bySeat = new WeakMap()
    merged.set(fishing, bySeat)
  }
  const cached = bySeat.get(seated)
  if (cached) return cached

  const tracks: THREE.KeyframeTrack[] = fishing.tracks.filter(
    (track) => !SEATED_BONES.has(boneOf(track.name))
  )
  for (const track of seated.tracks) {
    if (SEATED_BONES.has(boneOf(track.name)))
      tracks.push(holdFirstKeyframe(track))
  }

  const clip = new THREE.AnimationClip(
    fishing.name,
    fishing.duration,
    tracks,
    fishing.blendMode
  )
  bySeat.set(seated, clip)
  return clip
}
