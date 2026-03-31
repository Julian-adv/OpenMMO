import { MathUtils } from 'three'
import { get } from 'svelte/store'
import { gameStore, addChatMessage } from './stores/gameStore'
import { worldToTileCell } from './components/game-scene/terrain-utils'

type CommandHandler = (args: string) => void

const commands: Record<string, CommandHandler> = {
  '/pos': () => {
    const player = get(gameStore).currentPlayer
    if (player) {
      const pos = player.position
      const { tileX, tileZ, cellX, cellZ } = worldToTileCell(pos.x, pos.z)
      const deg = MathUtils.radToDeg(player.rotation).toFixed(1)
      addChatMessage({
        text: `Position: world(${pos.x.toFixed(1)}, ${pos.y.toFixed(1)}, ${pos.z.toFixed(1)}) tile(${tileX}, ${tileZ}) cell(${cellX}, ${cellZ}) rot(${deg}°)`,
        sender: 'system',
      })
    } else {
      addChatMessage({ text: 'Position: unknown', sender: 'system' })
    }
  },
}

export function handleCommand(input: string): boolean {
  const spaceIndex = input.indexOf(' ')
  const name = spaceIndex === -1 ? input : input.slice(0, spaceIndex)
  const args = spaceIndex === -1 ? '' : input.slice(spaceIndex + 1)
  const handler = commands[name]
  if (handler) {
    handler(args)
    return true
  }
  return false
}
