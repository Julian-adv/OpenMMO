export const DEFAULT_HAIR_COLOR = '#604332'
export const DEFAULT_EYE_COLOR = '#786545'

// Mean albedo of each female hair texture (matches the server).
export const FEMALE_HAIR_COLORS = {
  bob: '#ceb294',
  long: '#d2bda2',
  ponytail: '#432e26',
} as const
export const originalHairColor = (hair: string) =>
  FEMALE_HAIR_COLORS[hair as keyof typeof FEMALE_HAIR_COLORS] ??
  DEFAULT_HAIR_COLOR
const FEMALE_CLASS_HAIR: Record<string, keyof typeof FEMALE_HAIR_COLORS> = {
  ranger: 'ponytail',
  valkyrie: 'long',
}
export const femaleHairFor = (characterClass: string) =>
  FEMALE_CLASS_HAIR[characterClass] ?? 'bob'

export const HAIR_COLORS = [
  { value: '#211e1b', label: 'characterCreate.colorBlack' },
  { value: DEFAULT_HAIR_COLOR, label: 'characterCreate.colorBrown' },
  { value: '#a66b41', label: 'characterCreate.colorChestnut' },
  { value: '#d9b878', label: 'characterCreate.colorBlond' },
  { value: '#9c4630', label: 'characterCreate.colorAuburn' },
  { value: '#a9a8a3', label: 'characterCreate.colorGray' },
  { value: '#eee5d2', label: 'characterCreate.colorWhite' },
  { value: '#684575', label: 'characterCreate.colorPurple' },
] as const

export const EYE_COLORS = [
  { value: DEFAULT_EYE_COLOR, label: 'characterCreate.colorBrown' },
  { value: '#b58a3e', label: 'characterCreate.colorAmber' },
  { value: '#65815b', label: 'characterCreate.colorGreen' },
  { value: '#527baa', label: 'characterCreate.colorBlue' },
  { value: '#76b8bf', label: 'characterCreate.colorTeal' },
  { value: '#8d969c', label: 'characterCreate.colorGray' },
  { value: '#78518d', label: 'characterCreate.colorPurple' },
  { value: '#302824', label: 'characterCreate.colorDarkBrown' },
] as const
