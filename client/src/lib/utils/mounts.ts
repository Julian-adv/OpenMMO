import type { MountKind } from '../network/networkTypes'
import {
  mount_floats,
  mount_speed_mult,
  mount_turn_radius,
} from '../wasm/onlinerpg_shared'

/** Narrow the player and its mount together. */
export function isMounted<P extends { mount?: MountKind | null }>(
  player: P | null | undefined
): player is P & { mount: MountKind } {
  return player?.mount != null
}

interface MountSettings {
  speedMult: number
  turnRadius: number
  floats: boolean
}

const settingsByKind = new Map<MountKind, MountSettings>()

function mountSettings(mount: MountKind): MountSettings {
  let settings = settingsByKind.get(mount)
  if (!settings) {
    // Read shared constants lazily, after WASM initialization.
    settings = {
      speedMult: mount_speed_mult(mount),
      turnRadius: mount_turn_radius(mount),
      floats: mount_floats(mount),
    }
    settingsByKind.set(mount, settings)
  }
  return settings
}

export function mountSpeedMult(mount?: MountKind | null): number {
  return mount ? mountSettings(mount).speedMult : 1
}

export function mountTurnRadius(mount: MountKind): number {
  return mountSettings(mount).turnRadius
}

export function mountFloats(mount?: MountKind | null): boolean {
  return mount ? mountSettings(mount).floats : false
}
