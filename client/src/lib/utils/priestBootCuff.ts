import { createBootCuff } from './bootCuff'
import cuff from '../data/priestBootCuff.json'

export const {
  rim: priestBootRim,
  distance: priestPantsBootDistance,
  tuck: tuckPantsIntoPriestBoots,
} = createBootCuff(cuff)
