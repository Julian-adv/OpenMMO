import SkillTooltip from '../components/SkillTooltip.svelte'
import { createTooltipAction } from './tooltipAction'

export interface SkillTooltipParams {
  name: string
  description: string
  stats?: readonly { label: string; value: string }[]
  details?: readonly string[]
}

export const skillTooltip = createTooltipAction<SkillTooltipParams>(
  SkillTooltip,
  (params) => ({ ...params })
)
