const waiting: (() => void)[] = []
let scheduled = false

function scheduleNext() {
  if (scheduled || waiting.length === 0) return
  scheduled = true
  const afterFrame = () => {
    setTimeout(() => {
      scheduled = false
      waiting.shift()?.()
      scheduleNext()
    }, 0)
  }
  if (typeof requestAnimationFrame === 'function') {
    requestAnimationFrame(afterFrame)
  } else {
    afterFrame()
  }
}

/** Resume one preparation task after each paint, even when many models load. */
export function yieldTask(): Promise<void> {
  return new Promise((resolve) => {
    waiting.push(resolve)
    scheduleNext()
  })
}

/** Resolves at once until `budgetMs` of work has run since the last yield,
 *  then yields a task. */
export function createFrameYielder(budgetMs = 4): () => Promise<void> {
  let sliceStart = performance.now()
  return async () => {
    if (performance.now() - sliceStart < budgetMs) return
    await yieldTask()
    sliceStart = performance.now()
  }
}
