import { derived, get, writable } from 'svelte/store'
import shop from '../../../../data/furniture_shop.json'

export { shop as furnitureShop }
export type FurnitureProduct = (typeof shop.products)[number]
export type FurnitureBasketLine = { displayId: number; quantity: number }
export const furnitureBasket = writable<FurnitureBasketLine[]>([])
export const furnitureShopHover = writable<{
  displayId: number
  product: FurnitureProduct
} | null>(null)
export const furniturePurchasePending = writable(false)
export const furnitureShopError = writable<string | null>(null)
export const furnitureBasketTotal = derived(furnitureBasket, (lines) =>
  lines.reduce(
    (total, line) =>
      total + (displayProduct(line.displayId)?.price ?? 0) * line.quantity,
    0
  )
)

export const furnitureCart = derived(furnitureBasket, (lines) => {
  const entries = new Map<
    string,
    { kind: 'buy'; itemDefId: string; qty: number; unitPrice: number }
  >()
  for (const line of lines) {
    const product = displayProduct(line.displayId)
    if (!product) continue
    const entry = entries.get(product.itemDefId)
    if (entry) entry.qty += line.quantity
    else
      entries.set(product.itemDefId, {
        kind: 'buy',
        itemDefId: product.itemDefId,
        qty: line.quantity,
        unitPrice: product.price,
      })
  }
  return [...entries.values()]
})

export function displayProduct(displayId: number) {
  return shop.products.find((product) => product.displayIds.includes(displayId))
}

export function furnitureProduct(itemDefId: string) {
  return shop.products.find((product) => product.itemDefId === itemDefId)
}

export function addFurnitureItemToBasket(itemDefId: string) {
  const displayId = furnitureProduct(itemDefId)?.displayIds[0]
  if (displayId !== undefined) addFurnitureToBasket(displayId)
}

export function addFurnitureToBasket(displayId: number) {
  if (get(furniturePurchasePending) || !displayProduct(displayId)) return false
  const lines = get(furnitureBasket)
  if (lines.reduce((total, line) => total + line.quantity, 0) >= 64) {
    furnitureShopError.set('Your basket holds at most 64 pieces.')
    return false
  }
  const existing = lines.find((line) => line.displayId === displayId)
  furnitureBasket.set(
    existing
      ? lines.map((line) =>
          line === existing ? { ...line, quantity: line.quantity + 1 } : line
        )
      : [...lines, { displayId, quantity: 1 }]
  )
  furnitureShopError.set(null)
  return true
}

export function removeFurnitureFromBasket(displayId: number) {
  if (get(furniturePurchasePending)) return
  furnitureBasket.update((lines) =>
    lines.flatMap((line) =>
      line.displayId !== displayId
        ? [line]
        : line.quantity > 1
          ? [{ ...line, quantity: line.quantity - 1 }]
          : []
    )
  )
  furnitureShopError.set(null)
}

export function removeFurnitureItemFromBasket(itemDefId: string) {
  const line = get(furnitureBasket).find(
    (line) => displayProduct(line.displayId)?.itemDefId === itemDefId
  )
  if (line) removeFurnitureFromBasket(line.displayId)
}

export function clearFurnitureBasket() {
  furnitureBasket.set([])
  furnitureShopError.set(null)
}

export function resetFurnitureShop() {
  clearFurnitureBasket()
  furniturePurchasePending.set(false)
  furnitureShopHover.set(null)
}
