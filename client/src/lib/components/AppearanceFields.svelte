<script lang="ts">
  import {
    FACE_OPTIONS,
    FEMALE_FACE_OPTIONS,
    FEMALE_HAIR_OPTIONS,
    HAIR_OPTIONS,
  } from '../utils/characterAppearanceThumbnails'
  import {
    DEFAULT_EYE_COLOR,
    DEFAULT_HAIR_COLOR,
    EYE_COLORS,
    HAIR_COLORS,
    originalHairColor,
  } from '../utils/appearanceColors'
  import type { CharacterAppearance, Gender } from '../network/socket'
  import { t } from '../i18n'

  interface Props {
    gender: Gender
    appearance: CharacterAppearance
    onChange: (appearance: CharacterAppearance) => void
    disabled?: boolean
    onEyeColorPicked?: () => void
  }

  let {
    gender,
    appearance,
    onChange,
    disabled = false,
    onEyeColorPicked,
  }: Props = $props()

  let styleFields = $derived.by(() => {
    const female = gender === 'female'
    const hair = {
      key: 'hair' as const,
      label: 'characterCreate.hair' as const,
      value: appearance.hair,
      options: female ? FEMALE_HAIR_OPTIONS : HAIR_OPTIONS,
    }
    const face = {
      key: 'face' as const,
      label: 'characterCreate.face' as const,
      value: appearance.face,
      options: female ? FEMALE_FACE_OPTIONS : FACE_OPTIONS,
    }
    return [face, hair]
  })
  let colorFields = $derived([
    {
      key: 'hair_color' as const,
      label: 'characterCreate.hairColor' as const,
      value: appearance.hair_color_original
        ? originalHairColor(appearance.hair)
        : (appearance.hair_color ?? DEFAULT_HAIR_COLOR),
      original: appearance.hair_color_original ?? false,
      originalLabel: 'characterCreate.originalHairColor' as const,
      originalValue: originalHairColor(appearance.hair),
      options: HAIR_COLORS,
      disabled: appearance.hair === 'none',
      zoomFace: false,
    },
    {
      key: 'eye_color' as const,
      label: 'characterCreate.eyeColor' as const,
      value: appearance.eye_color ?? DEFAULT_EYE_COLOR,
      original: appearance.eye_color_original ?? false,
      originalLabel: 'characterCreate.originalEyeColor' as const,
      originalValue: DEFAULT_EYE_COLOR,
      options: EYE_COLORS,
      disabled: false,
      zoomFace: true,
    },
  ])

  function setColor(
    field: (typeof colorFields)[number],
    value: string,
    original: boolean
  ) {
    onChange({
      ...appearance,
      [field.key]: value,
      [`${field.key}_original`]: original,
    })
    if (field.zoomFace && !original) onEyeColorPicked?.()
  }
</script>

<div class="appearance-fields">
  {#each styleFields as field (field.key)}
    <fieldset class="appearance-field" {disabled}>
      <legend class="field-label">{$t(field.label)}</legend>
      <div class="appearance-grid">
        {#each field.options as option (option.value)}
          <button
            type="button"
            class="appearance-btn"
            class:appearance-selected={field.value === option.value}
            aria-label={$t(option.label)}
            aria-pressed={field.value === option.value}
            title={$t(option.label)}
            onclick={() =>
              onChange({
                ...appearance,
                [field.key]: option.value,
              })}
          >
            <img src={option.thumbnail} alt="" />
          </button>
        {/each}
      </div>
    </fieldset>
  {/each}
  {#each colorFields as field (field.key)}
    <fieldset class="appearance-field" disabled={disabled || field.disabled}>
      <legend class="field-label">{$t(field.label)}</legend>
      <button
        type="button"
        class="original-color-btn"
        class:appearance-selected={field.original}
        aria-pressed={field.original}
        onclick={() => setColor(field, field.originalValue, true)}
        >{$t(field.originalLabel)}</button
      >
      <div class="color-grid">
        {#each field.options as option (option.value)}
          <button
            type="button"
            class="color-swatch"
            class:appearance-selected={!field.original &&
              field.value === option.value}
            style:background-color={option.value}
            aria-label={$t(option.label)}
            aria-pressed={!field.original && field.value === option.value}
            title={$t(option.label)}
            onclick={() => setColor(field, option.value, false)}
          ></button>
        {/each}
      </div>
      <label class="custom-color">
        <span>{$t('characterCreate.customColor')}</span>
        <input
          type="color"
          value={field.value}
          aria-label={$t(field.label)}
          oninput={(event) => setColor(field, event.currentTarget.value, false)}
        />
      </label>
    </fieldset>
  {/each}
</div>

<style>
  .appearance-fields {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .field-label {
    font-size: 13px;
    color: #b8c6d9;
  }

  .appearance-field {
    margin: 0;
    padding: 0;
    border: 0;
  }

  .appearance-field legend {
    padding: 0;
    margin-bottom: 6px;
  }

  .appearance-grid {
    display: grid;
    grid-template-columns: repeat(2, 96px);
    gap: 8px;
  }

  .appearance-btn {
    box-sizing: border-box;
    width: 96px;
    height: 96px;
    border: 1px solid #526276;
    border-radius: 7px;
    padding: 0;
    overflow: hidden;
    background: #111923;
    color: #edf2f7;
    cursor: pointer;
  }

  .appearance-btn img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .appearance-btn:hover:not(:disabled),
  .appearance-btn:focus-visible {
    border-color: #8bbcff;
    outline: 2px solid #8bbcff;
    outline-offset: 2px;
  }

  .appearance-btn.appearance-selected {
    border-color: #2c7be5;
    background: #162a44;
    box-shadow: inset 0 0 0 1px #2c7be5;
  }

  .appearance-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .color-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }

  .original-color-btn {
    width: 100%;
    margin-bottom: 8px;
    padding: 6px 10px;
    border: 1px solid #61738a;
    border-radius: 5px;
    background: #111923;
    color: #edf2f7;
    cursor: pointer;
  }

  .original-color-btn.appearance-selected {
    border-color: #2c7be5;
    background: #162a44;
    box-shadow: inset 0 0 0 1px #2c7be5;
  }

  .original-color-btn:focus-visible {
    outline: 2px solid #8bbcff;
    outline-offset: 1px;
  }

  .color-swatch {
    height: 30px;
    border: 1px solid #61738a;
    border-radius: 5px;
    cursor: pointer;
  }

  .color-swatch.appearance-selected {
    outline: 2px solid #2c7be5;
    outline-offset: 1px;
  }

  .color-swatch:focus-visible {
    outline: 2px solid #8bbcff;
    outline-offset: 1px;
  }

  .custom-color {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: 8px;
    font-size: 12px;
    color: #b8c6d9;
  }

  .custom-color input {
    box-sizing: border-box;
    width: 48px;
    height: 28px;
    padding: 2px;
    border: 1px solid #61738a;
    border-radius: 4px;
    background: #111923;
    cursor: pointer;
  }

  .appearance-field:disabled .color-grid,
  .appearance-field:disabled .original-color-btn,
  .appearance-field:disabled .custom-color {
    opacity: 0.5;
  }

  @media (max-width: 600px), (max-height: 700px) {
    .field-label {
      font-size: 12px;
    }
  }
</style>
