/**
 * Shared work queue for terrain tile operations.
 * Both terrain height and water refresh tasks go through this single queue,
 * so the game loop processes at most N items per frame regardless of type.
 */

type WorkItem = () => void

const queue: WorkItem[] = []

/** Push a work item onto the shared queue. */
export function enqueueTileWork(fn: WorkItem): void {
  queue.push(fn)
}

/**
 * Process up to `maxItems` work items from the queue.
 * Called once per frame from the game loop.
 */
export function drainTileWork(maxItems = 1): void {
  for (let i = 0; i < maxItems && queue.length > 0; i++) {
    queue.shift()!()
  }
}
