// eslint-disable-next-line @typescript-eslint/no-explicit-any
type EventCallback = (...args: any[]) => void

export type NetworkEvent<T extends EventCallback> = {
  on: (handler: T) => () => void
  emit: (...args: Parameters<T>) => void
}

export function createEvent<T extends EventCallback>(): NetworkEvent<T> {
  const handlers = new Set<T>()
  return {
    on(handler: T) {
      handlers.add(handler)
      return () => {
        handlers.delete(handler)
      }
    },
    emit(...args: Parameters<T>) {
      handlers.forEach((h) => h(...args))
    },
  }
}
