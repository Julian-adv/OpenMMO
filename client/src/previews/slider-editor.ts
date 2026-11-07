/** One slider; lengths in metres show in cm. The save route reads the same list. */
export interface Slider {
  key: string
  label: string
  min: number
  max: number
  step: number
  cm?: boolean
}

/** A hidden panel of `sliders` with reset and save; `sliderEditor` wires it. */
export function sliderPanel(
  prefix: string,
  sliders: readonly Slider[],
  {
    note,
    saveLabel,
    legend,
  }: { note: string; saveLabel: string; legend: string }
) {
  return `
  <fieldset id="${prefix}" hidden>
    <legend>${legend}</legend>
    <p class="details">${note}</p>
    ${sliders
      .map(
        ({ key, label, min, max, step }) =>
          `<label for="${prefix}-${key}">${label} <output id="${prefix}-${key}-value"></output></label>
    <input id="${prefix}-${key}" type="range" min="${min}" max="${max}" step="${step}" />`
      )
      .join('\n    ')}
    <button id="${prefix}-reset" type="button">저장된 값으로</button>
    <button id="${prefix}-save" type="button">${saveLabel}</button>
    <p id="${prefix}-status" class="details" aria-live="polite"></p>
  </fieldset>`
}

/** Live values for the panel `prefix`: `apply` runs on every change, save
 *  posts them to `url` (with the `extra` fields of the moment) and keeps them
 *  as the reset point; `load` swaps in another set to edit. */
export function sliderEditor<T extends Record<string, number>>(
  prefix: string,
  sliders: readonly Slider[],
  initial: T,
  {
    url,
    apply,
    extra,
    onSaved,
  }: {
    url: string
    apply: (values: T) => void
    extra?: () => object
    onSaved?: (values: T) => void
  }
) {
  const panel = document.getElementById(prefix)!
  const status = document.getElementById(`${prefix}-status`)!
  let saved = { ...initial }
  const values = { ...initial }
  const input = (key: string) =>
    document.getElementById(`${prefix}-${key}`) as HTMLInputElement
  const show = () => {
    for (const { key, cm } of sliders) {
      const value = values[key]
      input(key).value = String(value)
      document.getElementById(`${prefix}-${key}-value`)!.textContent = cm
        ? `${(value * 100).toFixed(2)}cm`
        : `${value}°`
    }
    apply(values)
  }
  for (const { key } of sliders) {
    const slider = input(key)
    slider.oninput = () => {
      ;(values as Record<string, number>)[key] = Number(slider.value)
      show()
      status.textContent = '저장하지 않은 조절이 있습니다.'
    }
    // One wheel notch moves the slider one step.
    slider.addEventListener(
      'wheel',
      (event) => {
        event.preventDefault()
        if (event.deltaY === 0) return
        if (event.deltaY < 0) slider.stepUp()
        else slider.stepDown()
        slider.dispatchEvent(new Event('input'))
      },
      { passive: false }
    )
  }
  document.getElementById(`${prefix}-reset`)!.onclick = () => {
    Object.assign(values, saved)
    show()
    status.textContent = ''
  }
  document.getElementById(`${prefix}-save`)!.onclick = async () => {
    status.textContent = '저장하는 중…'
    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ ...extra?.(), ...values }),
      })
      const result = (await response.json()) as { error?: string }
      if (response.ok) {
        saved = { ...values }
        onSaved?.(saved)
      }
      status.textContent = response.ok
        ? '저장했습니다. 게임은 새로고침하면 반영됩니다.'
        : (result.error ?? '저장하지 못했습니다.')
    } catch {
      status.textContent = '저장하지 못했습니다.'
    }
  }
  show()
  return {
    panel,
    values,
    load(next: T) {
      Object.assign(values, next)
      saved = { ...next }
      show()
      status.textContent = ''
    },
  }
}
