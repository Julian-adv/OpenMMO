const INVENTORY_COLUMNS = 5
const INVENTORY_MIN_ROWS = 10

export function buildInventorySlots<T>(
  bag: readonly T[],
  columns = INVENTORY_COLUMNS,
  minRows = INVENTORY_MIN_ROWS
): (T | null)[] {
  const rows = Math.max(minRows, Math.ceil(bag.length / columns))

  return Array.from(
    { length: rows * columns },
    (_, index) => bag[index] ?? null
  )
}
