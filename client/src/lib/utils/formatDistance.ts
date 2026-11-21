/** Metres under a kilometre, tenths of a kilometre above. */
export function formatDistance(meters: number) {
  return meters >= 1000
    ? `${(meters / 1000).toFixed(1)} km`
    : `${Math.ceil(meters)} m`
}
