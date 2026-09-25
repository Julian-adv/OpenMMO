import { beforeEach, describe, expect, it } from 'vitest'
import { get } from 'svelte/store'
import { getItemDef } from '../data/itemDefs'
import {
  addFurnitureToBasket,
  addFurnitureItemToBasket,
  removeFurnitureFromBasket,
  removeFurnitureItemFromBasket,
  furnitureCart,
  clearFurnitureBasket,
  furnitureBasket,
  furnitureBasketTotal,
  furniturePurchasePending,
  furnitureShopError,
  furnitureShop,
  displayProduct,
  resetFurnitureShop,
} from './furnitureShopStore'

describe('showroom basket', () => {
  beforeEach(resetFurnitureShop)

  it('counts displayed decorations while excluding the store sign', () => {
    expect(addFurnitureToBasket(84)).toBe(false)
    expect(get(furnitureBasket)).toEqual([])
    expect(addFurnitureToBasket(103)).toBe(true)
    addFurnitureToBasket(103)
    addFurnitureToBasket(106)
    expect(get(furnitureBasketTotal)).toBe(600)
    removeFurnitureFromBasket(103)
    expect(get(furnitureBasketTotal)).toBe(400)
    removeFurnitureFromBasket(103)
    expect(get(furnitureBasket)).toEqual([{ displayId: 106, quantity: 1 }])
  })

  it('limits the basket across products and keeps pending orders stable', () => {
    for (let i = 0; i < 63; i++) addFurnitureToBasket(103)
    addFurnitureItemToBasket('furniture_iron_sword')
    addFurnitureItemToBasket('furniture_goblin_sword')
    expect(get(furnitureBasketTotal)).toBe(12800)
    expect(get(furnitureShopError)).not.toBeNull()
    expect(addFurnitureToBasket(94)).toBe(false)
    furniturePurchasePending.set(true)
    removeFurnitureFromBasket(103)
    expect(addFurnitureToBasket(103)).toBe(false)
    expect(get(furnitureBasketTotal)).toBe(12800)
    resetFurnitureShop()
    expect(get(furnitureBasket)).toEqual([])
    expect(get(furniturePurchasePending)).toBe(false)
    expect(get(furnitureShopError)).toBeNull()
  })

  it('combines menu and display selections while retaining checkout IDs', () => {
    addFurnitureItemToBasket('furniture_chair')
    addFurnitureToBasket(96)
    addFurnitureToBasket(96)
    addFurnitureToBasket(103)
    expect(get(furnitureCart)).toEqual([
      { kind: 'buy', itemDefId: 'furniture_chair', qty: 3, unitPrice: 800 },
      { kind: 'buy', itemDefId: 'furniture_scroll', qty: 1, unitPrice: 200 },
    ])
    expect(get(furnitureBasketTotal)).toBe(2600)

    removeFurnitureItemFromBasket('furniture_chair')
    expect(get(furnitureBasket)).toEqual([
      { displayId: 96, quantity: 2 },
      { displayId: 103, quantity: 1 },
    ])
    expect(get(furnitureCart)[0].qty).toBe(2)
    removeFurnitureItemFromBasket('furniture_chair')
    removeFurnitureItemFromBasket('furniture_chair')
    expect(get(furnitureCart)).toEqual([
      { kind: 'buy', itemDefId: 'furniture_scroll', qty: 1, unitPrice: 200 },
    ])
    expect(get(furnitureBasketTotal)).toBe(200)
  })

  it('keeps the checkout cart stable while paying and clears it with the basket', () => {
    addFurnitureToBasket(95)
    furniturePurchasePending.set(true)
    removeFurnitureItemFromBasket('furniture_chair')
    addFurnitureItemToBasket('furniture_chair')
    expect(get(furnitureCart)[0].qty).toBe(1)

    furniturePurchasePending.set(false)
    furnitureShopError.set('Your bag is too heavy.')
    expect(get(furnitureCart)[0].qty).toBe(1)
    removeFurnitureItemFromBasket('furniture_chair')
    expect(get(furnitureShopError)).toBeNull()
    addFurnitureToBasket(96)
    clearFurnitureBasket()
    expect(get(furnitureCart)).toEqual([])
  })

  it('can select every showroom product from the menu at the display price', () => {
    addFurnitureItemToBasket('furniture_chest')
    expect(get(furnitureBasket)).toEqual([])
    for (const product of furnitureShop.products) {
      expect(getItemDef(product.itemDefId)).toBeDefined()
      addFurnitureItemToBasket(product.itemDefId)
      expect(get(furnitureCart)).toContainEqual({
        kind: 'buy',
        itemDefId: product.itemDefId,
        qty: 1,
        unitPrice: product.price,
      })
    }
    expect(get(furnitureCart)).toHaveLength(furnitureShop.products.length)
  })

  it('sells the functional storage chest from both the display and menu', () => {
    expect(displayProduct(94)).toMatchObject({
      objectType: 'chest_animated',
      itemDefId: 'storage_chest',
      price: getItemDef('storage_chest')?.basePrice,
    })
    addFurnitureToBasket(94)
    addFurnitureItemToBasket('storage_chest')
    expect(get(furnitureCart)).toEqual([
      { kind: 'buy', itemDefId: 'storage_chest', qty: 2, unitPrice: 1200 },
    ])
    expect(get(furnitureBasketTotal)).toBe(2400)
  })
})
