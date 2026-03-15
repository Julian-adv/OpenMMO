/**
 * Shared work queue for terrain tile operations.
 * Both terrain height and water refresh tasks go through this single queue,
 * so the game loop processes items within a per-frame time budget.
 */

type WorkItem = () => void

let queue: WorkItem[] = []
let head = 0

/** Push a work item onto the shared queue. */
export function enqueueTileWork(fn: WorkItem): void {
  queue.push(fn)
}

/**
 * Process work items from the queue within a time budget.
 * Called once per frame from the game loop.
 * @param budgetMs - maximum milliseconds to spend per frame (default 4ms)
 */
export function drainTileWork(budgetMs = 4): void {
  if (head >= queue.length) return
  const deadline = performance.now() + budgetMs
  while (head < queue.length && performance.now() < deadline) {
    queue[head]()
    queue[head] = null! // allow GC
    head++
  }
  // Compact when fully drained or buffer gets large
  if (head >= queue.length) {
    queue = []
    head = 0
  } else if (head > 128) {
    queue = queue.slice(head)
    head = 0
  }
}
