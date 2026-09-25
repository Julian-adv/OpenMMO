import { describe, expect, it } from 'vitest'
import { stallTax } from './stallLimits'

describe('stall tax receipts', () => {
  it('rounds once on the total and stays exact at the safe integer limit', () => {
    expect(stallTax(0)).toBe(0)
    expect(stallTax(19)).toBe(0)
    expect(stallTax(20)).toBe(1)
    expect(stallTax(39)).toBe(1)
    expect(stallTax(1000)).toBe(50)
    expect(stallTax(Number.MAX_SAFE_INTEGER)).toBe(
      Number((BigInt(Number.MAX_SAFE_INTEGER) * 5n) / 100n)
    )
  })
})
