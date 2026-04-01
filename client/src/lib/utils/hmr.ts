export function hmrSingleton<T>(key: string, create: () => T): T {
  const instance = import.meta.hot?.data?.[key] ?? create()
  if (import.meta.hot) {
    import.meta.hot.dispose((data) => {
      data[key] = instance
    })
  }
  return instance
}
