import { createBootCuff } from './bootCuff'
import cuff from '../data/rangerBootCuff.json'

export const {
  rim: rangerBootRim,
  distance: rangerPantsBootDistance,
  tuck: tuckPantsIntoRangerBoots,
} = createBootCuff(cuff)
