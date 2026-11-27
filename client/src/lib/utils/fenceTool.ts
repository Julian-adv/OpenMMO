import { get } from 'svelte/store'
import { networkManager } from '../network/socket'
import { stopEstateChestMode } from '../stores/estateStorageStore'
import {
  landscapingMode,
  selectLandscapingTool,
} from '../stores/landscapingStore'

/** Switch the landscaping tools to fences, opening them first if needed. */
export function startFenceTool() {
  stopEstateChestMode()
  if (get(landscapingMode)) selectLandscapingTool('Fence')
  else networkManager.sendStartLandscapingMode('Fence')
}
