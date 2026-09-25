import type { KeyframeTrack } from 'three'

export function holdFirstKeyframe(track: KeyframeTrack): KeyframeTrack {
  const held = track.clone()
  held.times = new Float32Array([0])
  held.values = track.values.slice(0, track.getValueSize())
  return held
}
