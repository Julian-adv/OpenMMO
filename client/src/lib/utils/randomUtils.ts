/** Pick a random element from an array. Returns undefined if the array is empty. */
export function pickRandom<T>(arr: readonly T[]): T | undefined {
  if (arr.length === 0) return undefined
  return arr[Math.floor(Math.random() * arr.length)]
}
