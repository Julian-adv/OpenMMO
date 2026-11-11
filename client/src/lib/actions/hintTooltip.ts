import HintTooltip from '../components/HintTooltip.svelte'
import { createTooltipAction } from './tooltipAction'

/** A short text tip in the game's tooltip style, in place of `title`. */
export const hintTooltip = createTooltipAction<string>(HintTooltip, (text) => ({
  text,
}))
