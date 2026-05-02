import { getTerrainApiUrl } from '../utils/networkUtils'

/** Build the server URL for a region minimap (HTTP-cacheable). */
export function regionMinimapServerUrl(rx: number, rz: number): string {
  return `${getTerrainApiUrl()}/api/terrain/minimap/${rx}/${rz}`
}
