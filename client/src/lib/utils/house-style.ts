import { HOUSING_TEXTURES } from './housing-textures'

function byLabel(label: string): number {
  const index = HOUSING_TEXTURES.findIndex((e) => e.label === label)
  if (index < 0) throw new Error(`housing texture "${label}" is missing`)
  return index
}

/** Textures shared by every dressed set. */
export const STYLE_TEX = {
  frame: byLabel('Timber'),
  stone: byLabel('Fieldstone'),
  plaster: byLabel('Cream Plaster'),
  door: byLabel('Plank Door'),
  sash: byLabel('Timber Sash'),
  decor: byLabel('Storybook Decor'),
}

/** Texture V per roof course: two rows of the six-row tile images. */
export const ROOF_COURSE_V = 0.5

/** sRGB tints over the set textures. */
export interface HousePalette {
  plaster: number
  roof: number
  shutter: number
}

export interface HouseStyle {
  label: string
  roofTexture: number
  /** Shutters, flower boxes, lit windows and door lanterns. */
  decor: boolean
  ivy: boolean
  /** Stone ground storey without the timber frame, and a door step. */
  stoneGround: boolean
  palettes: HousePalette[]
}

// Indexed by HouseData.style; the length must equal shared
// housing::HOUSE_STYLE_COUNT.
const HOUSE_STYLES: (HouseStyle | null)[] = [
  null,
  {
    label: 'Timber',
    roofTexture: byLabel('Beaver-tail Tiles'),
    decor: false,
    ivy: false,
    stoneGround: false,
    palettes: [{ plaster: 0xffffff, roof: 0xffffff, shutter: 0xffffff }],
  },
  {
    label: 'Storybook',
    roofTexture: byLabel('Beaver-tail Tiles'),
    decor: true,
    ivy: true,
    stoneGround: false,
    palettes: [
      { plaster: 0xffffff, roof: 0xffffff, shutter: 0x4f7a5a },
      { plaster: 0xfff0c4, roof: 0xd9c2b0, shutter: 0x3f6f8f },
      { plaster: 0xffdcd2, roof: 0xfff2e0, shutter: 0x8a3a34 },
      { plaster: 0xe2efd8, roof: 0xc9b8a8, shutter: 0x6a4a7a },
      { plaster: 0xdcebf5, roof: 0xe8d6c4, shutter: 0x2f6f6a },
    ],
  },
  {
    label: 'Stone',
    roofTexture: byLabel('Slate Tiles'),
    decor: true,
    ivy: false,
    stoneGround: true,
    palettes: [
      { plaster: 0xffffff, roof: 0xffffff, shutter: 0x5b7a62 },
      { plaster: 0xfff4dc, roof: 0xdfe6ee, shutter: 0x4d6f8c },
      { plaster: 0xf4efe6, roof: 0xe9e2da, shutter: 0x6b7480 },
    ],
  },
]

export const HOUSE_STYLE_LABELS = HOUSE_STYLES.map((s) => s?.label ?? 'Plain')

export type ResolvedHouseStyle = HouseStyle & {
  palette: HousePalette
  /** Hash of the house id, for per-piece choices that must stay put. */
  seed: number
}

// Dev look-dev: ?houseStyle=N draws every plain house in set N.
const previewStyle = import.meta.env.DEV
  ? Number(
      new URLSearchParams(globalThis.location?.search ?? '').get('houseStyle')
    ) || 0
  : 0

function hashId(id: string): number {
  let h = 2166136261
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  return h >>> 0
}

export function resolveHouseStyle(
  id: number | undefined,
  houseId: string
): ResolvedHouseStyle | null {
  const style = HOUSE_STYLES[id || previewStyle]
  if (!style) return null
  const seed = hashId(houseId)
  return {
    ...style,
    seed,
    palette: style.palettes[seed % style.palettes.length],
  }
}
