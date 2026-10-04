<script lang="ts">
  import {
    FACE_OPTIONS,
    HAIR_OPTIONS,
  } from '../utils/characterAppearanceThumbnails'
  import {
    DEFAULT_EYE_COLOR,
    DEFAULT_HAIR_COLOR,
    EYE_COLORS,
    HAIR_COLORS,
  } from '../utils/appearanceColors'
  import type {
    AccountCharacter,
    CharacterClass,
    CharacterAppearance,
    CharacterRollResult,
    Gender,
    RollCharacterStatsResult,
  } from '../network/socket'
  import { t } from '../i18n'
  import {
    getAvailableGenders,
    getCharacterModelPath,
    MODULAR_MALE_MODEL_PATH,
  } from '../utils/modelPaths'

  const MAX_CHARACTER_SLOTS = 3
  const CHARACTER_CLASSES = [
    'knight',
    'barbarian',
    'rogue',
    'caveman',
    'valkyrie',
    'ranger',
    'priest',
    'bard',
  ] as const satisfies readonly CharacterClass[]

  interface Props {
    accountName: string
    characters: AccountCharacter[]
    selectedClass: CharacterClass
    selectedGender: Gender
    appearance: CharacterAppearance
    onAppearanceChange: (appearance: CharacterAppearance) => void
    facePreview?: boolean
    onFacePreviewChange?: (zoomed: boolean) => void
    onClassChange: (cls: CharacterClass) => void
    onGenderChange: (gender: Gender) => void
    onRollCharacterStats: (
      cls: CharacterClass,
      gender: Gender
    ) => Promise<RollCharacterStatsResult>
    onCreateCharacter: (
      characterName: string,
      characterClass: CharacterClass,
      gender: Gender,
      appearance: CharacterAppearance
    ) => Promise<{
      ok: boolean
      message?: string
      character?: AccountCharacter
    }>
    onCharacterCreated: (characterId: number) => void
    onCancel: () => void
  }

  let {
    accountName,
    characters,
    selectedClass,
    selectedGender,
    appearance,
    onAppearanceChange,
    facePreview = false,
    onFacePreviewChange,
    onClassChange,
    onGenderChange,
    onRollCharacterStats,
    onCreateCharacter,
    onCharacterCreated,
    onCancel,
  }: Props = $props()

  let availableGenders = $derived(getAvailableGenders(selectedClass))
  let canCustomize = $derived(
    getCharacterModelPath(selectedClass, selectedGender) ===
      MODULAR_MALE_MODEL_PATH
  )
  let styleFields = $derived([
    {
      key: 'face' as const,
      label: 'characterCreate.face' as const,
      value: appearance.face,
      options: FACE_OPTIONS,
    },
    {
      key: 'hair' as const,
      label: 'characterCreate.hair' as const,
      value: appearance.hair,
      options: HAIR_OPTIONS,
    },
  ])
  let colorFields = $derived([
    {
      key: 'hair_color' as const,
      label: 'characterCreate.hairColor' as const,
      value: appearance.hair_color ?? DEFAULT_HAIR_COLOR,
      options: HAIR_COLORS,
      disabled: appearance.hair === 'none',
      zoomFace: false,
    },
    {
      key: 'eye_color' as const,
      label: 'characterCreate.eyeColor' as const,
      value: appearance.eye_color ?? DEFAULT_EYE_COLOR,
      options: EYE_COLORS,
      disabled: false,
      zoomFace: true,
    },
  ])
  let createCharacterName = $state('')
  let rolledStats = $state<CharacterRollResult | null>(null)
  let isCreating = $state(false)
  let isRolling = $state(false)
  let errorMessage = $state('')

  function isBusy() {
    return isCreating || isRolling
  }

  function selectColor(field: (typeof colorFields)[number], value: string) {
    onAppearanceChange({ ...appearance, [field.key]: value })
    if (field.zoomFace) onFacePreviewChange?.(true)
  }

  function atSlotLimit() {
    return characters.length >= MAX_CHARACTER_SLOTS
  }

  function selectClass(cls: CharacterClass) {
    onClassChange(cls)
    const genders = getAvailableGenders(cls)
    if (!genders.includes(selectedGender)) {
      onGenderChange(genders[0])
    }
    rolledStats = null
  }

  function selectGender(g: Gender) {
    onGenderChange(g)
    rolledStats = null
  }

  async function handleRoll() {
    if (isBusy()) return
    if (atSlotLimit()) {
      errorMessage = $t('characterCreate.slotLimit', {
        max: MAX_CHARACTER_SLOTS,
      })
      return
    }

    isRolling = true
    errorMessage = ''
    const result = await onRollCharacterStats(selectedClass, selectedGender)
    isRolling = false

    if (!result.ok) {
      errorMessage = result.message
      return
    }

    rolledStats = { attributes: result.attributes, maxHp: result.maxHp }
  }

  async function submitCreateCharacter(event: Event) {
    event.preventDefault()
    if (isBusy()) return

    if (atSlotLimit()) {
      errorMessage = $t('characterCreate.slotLimit', {
        max: MAX_CHARACTER_SLOTS,
      })
      return
    }

    const characterName = createCharacterName.trim()
    if (!characterName) {
      errorMessage = $t('characterCreate.nameRequired')
      return
    }
    if (!rolledStats) {
      errorMessage = $t('characterCreate.rollRequired')
      return
    }

    isCreating = true
    errorMessage = ''
    const result = await onCreateCharacter(
      characterName,
      selectedClass,
      selectedGender,
      canCustomize ? appearance : { face: 'default', hair: 'crop' }
    )
    isCreating = false

    if (!result.ok) {
      errorMessage = result.message ?? $t('characterCreate.createFailed')
      return
    }

    if (!result.character) {
      errorMessage = $t('characterCreate.missingCharacterData')
      return
    }

    createCharacterName = ''
    rolledStats = null
    onCharacterCreated(result.character.id)
  }
</script>

<div class="character-create-overlay">
  <div class="top-bar">
    <h1 class="title">{$t('characterCreate.title')}</h1>
    <p class="account-name">{$t('characterSelect.account')}: {accountName}</p>
  </div>

  <form class="create-form" onsubmit={submitCreateCharacter}>
    <div class="class-column">
      <span class="field-label">{$t('characterCreate.class')}</span>
      {#each CHARACTER_CLASSES as cls (cls)}
        <button
          type="button"
          class="class-btn"
          class:class-selected={selectedClass === cls}
          disabled={isBusy()}
          onclick={() => selectClass(cls)}
        >
          {$t(
            cls === 'caveman' && selectedGender === 'female'
              ? 'class.cavewoman'
              : `class.${cls}`
          )}
        </button>
      {/each}
    </div>

    {#if canCustomize}
      <section class="appearance-panel" aria-labelledby="appearance-title">
        <h2 id="appearance-title" class="appearance-title">
          {$t('characterCreate.appearance')}
        </h2>
        {#each styleFields as field (field.key)}
          <fieldset class="appearance-field" disabled={isBusy()}>
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
                    onAppearanceChange({
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
          <fieldset
            class="appearance-field"
            disabled={isBusy() || field.disabled}
          >
            <legend class="field-label">{$t(field.label)}</legend>
            <div class="color-grid">
              {#each field.options as option (option.value)}
                <button
                  type="button"
                  class="color-swatch"
                  class:appearance-selected={field.value === option.value}
                  style:background-color={option.value}
                  aria-label={$t(option.label)}
                  aria-pressed={field.value === option.value}
                  title={$t(option.label)}
                  onclick={() => selectColor(field, option.value)}
                ></button>
              {/each}
            </div>
            <label class="custom-color">
              <span>{$t('characterCreate.customColor')}</span>
              <input
                type="color"
                value={field.value}
                aria-label={$t(field.label)}
                oninput={(event) =>
                  selectColor(field, event.currentTarget.value)}
              />
            </label>
          </fieldset>
        {/each}
        <button
          type="button"
          class="secondary face-preview-btn"
          aria-pressed={facePreview}
          onclick={() => onFacePreviewChange?.(!facePreview)}
        >
          {$t(
            facePreview
              ? 'characterCreate.fullBody'
              : 'characterCreate.zoomFace'
          )}
        </button>
      </section>
    {/if}

    <div class="bottom-bar">
      {#if errorMessage}
        <div class="error-message">{errorMessage}</div>
      {/if}

      <div class="bottom-row">
        <div class="gender-field">
          <span class="field-label">{$t('characterCreate.gender')}</span>
          <div class="gender-buttons">
            <button
              type="button"
              class="class-btn"
              class:class-selected={selectedGender === 'male'}
              disabled={isBusy() || !availableGenders.includes('male')}
              onclick={() => selectGender('male')}
            >
              {$t('characterCreate.male')}
            </button>
            <button
              type="button"
              class="class-btn"
              class:class-selected={selectedGender === 'female'}
              disabled={isBusy() || !availableGenders.includes('female')}
              onclick={() => selectGender('female')}
            >
              {$t('characterCreate.female')}
            </button>
          </div>
        </div>

        <label class="name-field" for="characterName">
          <span class="field-label">{$t('characterCreate.name')}</span>
          <input
            id="characterName"
            type="text"
            bind:value={createCharacterName}
            maxlength={24}
            placeholder={$t('characterCreate.namePlaceholder')}
            disabled={isBusy()}
          />
        </label>

        <div
          class="rolled-attributes"
          role="button"
          tabindex="0"
          onclick={handleRoll}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') handleRoll()
          }}
        >
          {#if rolledStats}
            <div class="attr">
              {$t('stat.str')}
              {rolledStats.attributes.str}
            </div>
            <div class="attr">
              {$t('stat.dex')}
              {rolledStats.attributes.dex}
            </div>
            <div class="attr">
              {$t('stat.con')}
              {rolledStats.attributes.con}
            </div>
            <div class="attr">
              {$t('stat.int')}
              {rolledStats.attributes.int}
            </div>
            <div class="attr">
              {$t('stat.wis')}
              {rolledStats.attributes.wis}
            </div>
            <div class="attr">
              {$t('stat.cha')}
              {rolledStats.attributes.cha}
            </div>
            <div class="attr">{$t('stat.hp')} {rolledStats.maxHp}</div>
          {:else}
            <div class="roll-hint">
              {$t('characterCreate.rollHint')}
            </div>
          {/if}
        </div>

        <div class="create-actions">
          <button
            type="button"
            class="secondary"
            disabled={isBusy()}
            onclick={handleRoll}
          >
            {isRolling
              ? $t('characterCreate.rolling')
              : $t('characterCreate.roll')}
          </button>
          <button
            type="submit"
            class="primary"
            disabled={isBusy() || !rolledStats || atSlotLimit()}
          >
            {isCreating
              ? $t('characterCreate.creating')
              : $t('characterCreate.create')}
          </button>
          <button
            type="button"
            class="secondary"
            disabled={isBusy()}
            onclick={onCancel}
          >
            {$t('common.cancel')}
          </button>
        </div>
      </div>
    </div>
  </form>
</div>

<style>
  .character-create-overlay {
    position: fixed;
    inset: 0;
    box-sizing: border-box;
    width: 100%;
    max-width: 100vw;
    height: 100vh;
    height: 100dvh;
    overflow: hidden;
    z-index: 1;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    color: #edf2f7;
    pointer-events: none;
  }

  .top-bar {
    text-align: center;
    padding: max(24px, calc(env(safe-area-inset-top) + 12px)) 16px 0;
  }

  .title {
    margin: 0;
    font-size: 28px;
    text-shadow: 0 2px 8px rgba(0, 0, 0, 0.6);
  }

  .account-name {
    margin: 6px 0 0;
    color: #9fb0c6;
    font-size: 13px;
    text-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
  }

  .create-form {
    position: fixed;
    left: 16px;
    bottom: max(16px, calc(env(safe-area-inset-bottom) + 10px));
    max-width: calc(100vw - 72px);
    max-height: calc(100dvh - 92px);
    display: flex;
    align-items: flex-end;
    gap: 10px;
    pointer-events: auto;
  }

  .field-label {
    font-size: 13px;
    color: #b8c6d9;
  }

  .class-column {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 120px;
  }

  .bottom-bar {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .bottom-row {
    display: flex;
    align-items: end;
    gap: 10px;
  }

  .appearance-panel {
    position: fixed;
    top: 50%;
    right: max(16px, calc(env(safe-area-inset-right) + 10px));
    max-height: calc(100dvh - 104px - env(safe-area-inset-top));
    box-sizing: border-box;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 10px;
    border: 1px solid #45556b;
    border-radius: 8px;
    background: rgba(16, 24, 35, 0.9);
    transform: translateY(-50%);
  }

  .appearance-title {
    margin: 0;
    font-size: 16px;
    color: #edf2f7;
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
  .appearance-field:disabled .custom-color {
    opacity: 0.5;
  }

  .face-preview-btn {
    border-radius: 7px;
    padding: 7px 10px;
    font-size: 13px;
    cursor: pointer;
  }

  .gender-field {
    display: grid;
    gap: 6px;
    min-width: 160px;
  }

  .gender-buttons {
    display: flex;
    gap: 6px;
  }

  .gender-buttons .class-btn {
    height: 34px;
    padding: 6px 12px;
  }

  .class-btn {
    flex: 1;
    border: 1px solid #526276;
    border-radius: 7px;
    padding: 10px 12px;
    background: #111923;
    color: #9fb0c6;
    font-size: 14px;
    cursor: pointer;
    transition:
      background 120ms ease,
      color 120ms ease,
      border-color 120ms ease;
  }

  .class-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .class-btn.class-selected {
    border-color: #2c7be5;
    background: #162a44;
    color: #edf2f7;
    font-weight: 600;
  }

  .name-field {
    flex: 1;
    display: grid;
    gap: 6px;
  }

  .name-field input {
    border: 1px solid #526276;
    border-radius: 7px;
    height: 34px;
    padding: 6px 12px;
    background: #111923;
    color: #f7fafc;
    font-size: 14px;
    box-sizing: border-box;
  }

  .rolled-attributes {
    width: 220px;
    border: 1px solid #45556b;
    border-radius: 8px;
    background: rgba(16, 24, 35, 0.9);
    padding: 10px;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
    height: 90px;
    align-items: center;
    cursor: pointer;
  }

  .attr {
    font-size: 13px;
    font-weight: 600;
    color: #e4ecf5;
    text-align: center;
  }

  .roll-hint {
    grid-column: 1 / -1;
    font-size: 12px;
    color: #9fb0c6;
    text-align: center;
  }

  .create-actions {
    display: flex;
    align-items: end;
    gap: 10px;
    margin-left: auto;
  }

  .create-actions button {
    flex: 1 0 auto;
    border-radius: 7px;
    height: 34px;
    padding: 6px 12px;
    font-size: 14px;
    line-height: 1.2;
    white-space: nowrap;
    cursor: pointer;
  }

  .create-actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .primary {
    border: none;
    background: #2c7be5;
    color: white;
    font-weight: 600;
  }

  .secondary {
    border: 1px solid #61738a;
    background: #1c2736;
    color: #dbe6f2;
  }

  .error-message {
    border: 1px solid #f28b8b;
    border-radius: 7px;
    padding: 10px 12px;
    background: rgba(175, 45, 45, 0.2);
    color: #ffd2d2;
    font-size: 13px;
    text-align: center;
  }

  @media (max-width: 1100px) {
    .bottom-row {
      flex-direction: column;
    }

    .create-actions {
      margin-left: 0;
    }
  }

  @media (max-width: 600px), (max-height: 700px) {
    .appearance-panel {
      right: max(10px, calc(env(safe-area-inset-right) + 8px));
    }

    .top-bar {
      padding-top: max(14px, calc(env(safe-area-inset-top) + 8px));
    }

    .title {
      font-size: 22px;
    }

    .account-name {
      margin-top: 3px;
      font-size: 12px;
    }

    .create-form {
      left: 10px;
      bottom: max(10px, calc(env(safe-area-inset-bottom) + 8px));
      max-width: calc(100vw - 62px);
      max-height: calc(100dvh - 68px);
      gap: 8px;
    }

    .field-label {
      font-size: 12px;
    }

    .class-column {
      gap: 4px;
      min-width: 104px;
    }

    .class-btn {
      padding: 7px 9px;
      font-size: 13px;
      line-height: 1.15;
    }

    .bottom-bar {
      gap: 8px;
    }

    .bottom-row {
      gap: 8px;
    }

    .gender-field {
      gap: 4px;
      min-width: 134px;
    }

    .gender-buttons {
      gap: 5px;
    }

    .gender-buttons .class-btn,
    .name-field input,
    .create-actions button {
      height: 30px;
    }

    .name-field {
      gap: 4px;
    }

    .name-field input {
      padding: 5px 9px;
      font-size: 13px;
    }

    .rolled-attributes {
      width: 180px;
      height: 72px;
      padding: 8px;
      gap: 4px;
    }

    .attr {
      font-size: 12px;
    }

    .roll-hint {
      font-size: 11px;
    }

    .create-actions {
      gap: 6px;
    }

    .create-actions button {
      padding: 5px 9px;
      font-size: 13px;
    }
  }

  @media (max-height: 560px) {
    .top-bar {
      padding-top: max(8px, env(safe-area-inset-top));
    }

    .title {
      font-size: 18px;
    }

    .account-name {
      display: none;
    }

    .class-column {
      gap: 3px;
    }

    .class-btn {
      padding-top: 6px;
      padding-bottom: 6px;
    }
  }

  @media (max-width: 600px) {
    .appearance-panel {
      max-height: max(96px, calc(100dvh - 520px));
    }
  }
</style>
