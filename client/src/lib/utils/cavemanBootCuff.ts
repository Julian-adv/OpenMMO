import { createBootCuff } from './bootCuff'
import cuff from '../data/cavemanBootCuff.json'

export const {
  rim: cavemanBootRim,
  distance: cavemanPantsBootDistance,
  tuck: tuckPantsIntoCavemanBoots,
} = createBootCuff(cuff)
