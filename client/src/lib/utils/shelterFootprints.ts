import type { ObjectDef, ObjectPlacement } from '../stores/editorStore'
import type { Position } from '../network/networkTypes'
import { isInsideShelter } from './shelter'

export type ShelterFootprint = {
  placement: ObjectPlacement
  perimeter: NonNullable<ObjectDef['perimeter']>
}

const byOwner = new Map<object, ShelterFootprint[]>()

/** Each object overlay reports the walled buildings it shows. */
export function setShelterFootprints(
  owner: object,
  footprints: ShelterFootprint[] | null
) {
  if (footprints) byOwner.set(owner, footprints)
  else byOwner.delete(owner)
}

export function shelterAt(point: Position): ShelterFootprint | undefined {
  for (const footprints of byOwner.values()) {
    const found = footprints.find((f) =>
      isInsideShelter(point, f.placement, f.perimeter)
    )
    if (found) return found
  }
  return undefined
}
