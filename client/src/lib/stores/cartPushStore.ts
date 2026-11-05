import { writable } from 'svelte/store'

export const CART_PUSH_PACK = 'animations/cart_push.glb'
export const CART_PUSH_CLIP = 'cart_push'

/** Player id → when their latest cart push was broadcast. */
export const cartPushes = writable(new Map<number, number>())

export function recordCartPush(playerId: number) {
  const now = Date.now()
  cartPushes.update((pushes) => {
    const next = new Map([...pushes].filter(([, at]) => now - at < 5000))
    next.set(playerId, now)
    return next
  })
}
