// Node's navigator follows the host LANG; tests expect an English browser.
if (typeof navigator !== 'undefined') {
  Object.defineProperties(navigator, {
    language: { value: 'en-US', configurable: true },
    languages: { value: ['en-US'], configurable: true },
  })
}
