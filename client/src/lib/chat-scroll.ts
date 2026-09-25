const BOTTOM_TOLERANCE_PX = 8

export interface ScrollPosition {
  scrollHeight: number
  scrollTop: number
  clientHeight: number
}

export function isChatAtBottom(position: ScrollPosition): boolean {
  return (
    position.scrollHeight - position.scrollTop - position.clientHeight <=
    BOTTOM_TOLERANCE_PX
  )
}
