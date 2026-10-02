import { translate, type Locale, type MessageKey } from './index'

const reasons = new Map<string, MessageKey>([
  ['This Land Deed is reserved for trade.', 'landClaim.reason.tradeReserved'],
  ['Character not found.', 'landClaim.reason.characterMissing'],
  ['Inventory not found.', 'landClaim.reason.inventoryMissing'],
  [
    'Land registration is temporarily unavailable.',
    'landClaim.reason.unavailable',
  ],
  ['This plot has an invalid land grade.', 'landClaim.reason.invalidGrade'],
  [
    'Crown land cannot be claimed with a Land Deed.',
    'landClaim.reason.crownLand',
  ],
  [
    'This plot is reserved and cannot be claimed.',
    'landClaim.reason.reservedPlot',
  ],
  ['Land Deed not found in your bag.', 'landClaim.reason.deedMissing'],
  [
    'Land registration could not be saved. Your Land Deed was not consumed.',
    'landClaim.reason.saveFailed',
  ],
  [
    'Land registration could not check existing houses. Your Land Deed was not consumed.',
    'landClaim.reason.houseCheckFailed',
  ],
  ['You must be alive to claim land.', 'landClaim.reason.defeated'],
  [
    'Stand on outdoor ground to claim land.',
    'landClaim.reason.outdoorsRequired',
  ],
  [
    'This location is outside the claimable world.',
    'landClaim.reason.outsideWorld',
  ],
  [
    'You left the selected plot. Use the Land Deed again.',
    'landClaim.reason.plotChanged',
  ],
  ['This plot already belongs to someone.', 'landClaim.reason.occupied'],
  [
    'Another character on your account already owns a homestead.',
    'landClaim.reason.otherCharacter',
  ],
  [
    'Your homestead has reached its limit of 16 plots.',
    'landClaim.reason.plotLimit',
  ],
  [
    'Choose a plot that shares an edge with your homestead.',
    'landClaim.reason.adjacentRequired',
  ],
  [
    'Your homestead must fit within an 8 by 8 plot area.',
    'landClaim.reason.areaLimit',
  ],
])

export function landClaimReason(
  reason: string | undefined,
  language?: Locale
): string {
  if (!reason) return ''
  const levelRequirement = reason.match(
    /^You must be level (\d+) or higher to use a Land Deed\.$/
  )
  if (levelRequirement) {
    return translate(
      'landClaim.reason.levelRequired',
      { level: Number(levelRequirement[1]) },
      language
    )
  }
  const key = reasons.get(reason)
  return key ? translate(key, {}, language) : reason
}
