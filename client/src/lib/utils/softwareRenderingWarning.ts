import { get } from 'svelte/store'
import { translate } from '../i18n'
import { showAnnouncement } from '../stores/announcementStore'
import { addChatMessage } from '../stores/gameStore'
import {
  graphicsQuality,
  softwareCapsQuality,
} from '../stores/graphicsSettings'
import { detectSoftwareRenderer } from './softwareRenderer'

/** Tell the player in a toast and in chat that the game is drawing without
 *  the GPU. */
export function warnSoftwareRendering(): void {
  if (!detectSoftwareRenderer()) return
  const text = translate(
    softwareCapsQuality(get(graphicsQuality))
      ? 'graphics.softwareRenderingLowered'
      : 'graphics.softwareRendering'
  )
  showAnnouncement(text, translate('graphics.softwareRenderingTitle'))
  addChatMessage({ text, sender: 'system' })
}
