/** Resolve in a later task so long-running work lets frames render. */
export function yieldTask(): Promise<void> {
  const scheduler = (
    globalThis as { scheduler?: { yield?: () => Promise<void> } }
  ).scheduler
  // setTimeout is clamped to 4 ms once nested; scheduler.yield is not.
  if (scheduler?.yield) return scheduler.yield()
  return new Promise((r) => setTimeout(r, 0))
}

/** Resolves at once until `budgetMs` of work has run since the last yield,
 *  then yields a task. */
export function createFrameYielder(budgetMs = 6): () => Promise<void> {
  let sliceStart = performance.now()
  return async () => {
    if (performance.now() - sliceStart < budgetMs) return
    await yieldTask()
    sliceStart = performance.now()
  }
}
