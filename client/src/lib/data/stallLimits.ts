/** Mirrors shared/src/stall.rs. */
export const STALL_MAX_LISTINGS = 12
export const STALL_TAX_PERCENT = 5

export function stallTax(total: number): number {
  return (
    Math.floor(total / 100) * STALL_TAX_PERCENT +
    Math.floor(((total % 100) * STALL_TAX_PERCENT) / 100)
  )
}
