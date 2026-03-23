<script lang="ts">
  import { onMount } from 'svelte'
  import {
    ANIMATION_PACK_PATH_HINT,
    getDefaultPackName,
    getDefaultPacks,
    loadAnimationPackCatalog,
  } from '../animation-pack-api'

  interface Props {
    clipNames: string[]
    selectedClipIndex: number
    onExtract: (packName: string, clipName: string, packFilesByName: Record<string, string>) => Promise<void>
    onClose: () => void
  }

  let { clipNames, selectedClipIndex, onExtract, onClose }: Props = $props()

  let animationPacks = $state<string[]>(getDefaultPacks())
  let packFilesByName = $state<Record<string, string>>({})
  let selectedPackName = $state(getDefaultPackName())
  let extractClipName = $state(clipNames[selectedClipIndex] ?? 'Animation')
  let isLoadingPackCatalog = $state(false)
  let isExtracting = $state(false)

  const trimmedSelectedPackName = $derived(selectedPackName.trim())
  const trimmedExtractClipName = $derived(extractClipName.trim())
  const selectedPackFileName = $derived(packFilesByName[trimmedSelectedPackName] ?? '')
  const selectedPackExists = $derived(selectedPackFileName !== '')
  const canExtract = $derived(
    clipNames.length > 0 && trimmedSelectedPackName !== '' && trimmedExtractClipName !== '',
  )

  async function refreshPackCatalog(): Promise<void> {
    isLoadingPackCatalog = true
    try {
      const catalog = await loadAnimationPackCatalog()
      packFilesByName = catalog.packFilesByName
      animationPacks = catalog.animationPacks
      if (!animationPacks.includes(selectedPackName)) {
        selectedPackName = animationPacks[0] ?? getDefaultPackName()
      }
    } catch {
      packFilesByName = {}
      animationPacks = getDefaultPacks()
      if (!animationPacks.includes(selectedPackName)) {
        selectedPackName = animationPacks[0] ?? ''
      }
    } finally {
      isLoadingPackCatalog = false
    }
  }

  async function handleExtract(): Promise<void> {
    if (!canExtract) return
    isExtracting = true
    try {
      await onExtract(trimmedSelectedPackName, trimmedExtractClipName, packFilesByName)
    } finally {
      isExtracting = false
    }
  }

  onMount(() => {
    void refreshPackCatalog()
  })
</script>

<div class="dialog-overlay">
  <div class="extract-panel">
    <div class="extract-panel-header">
      <h2>애니메이션 추출</h2>
      <div class="spacer"></div>
      <button class="btn ghost" onclick={onClose}>닫기</button>
    </div>

    <div class="extract-panel-body">
      <p class="small path-hint">권장 저장 위치: {ANIMATION_PACK_PATH_HINT}</p>
      <label class="small full-width">
        <span class="lbl-prefix">클립 이름</span>
        <input class="clip-name-input" type="text" bind:value={extractClipName} placeholder="추출할 클립 이름" />
      </label>

      <div class="small">애니메이션 팩 선택</div>
      <div class="pack-list">
        {#each animationPacks as pack (pack)}
          <label class="pack-item">
            <input type="radio" name="animation-pack" value={pack} bind:group={selectedPackName} />
            <span>{pack}</span>
          </label>
        {/each}
      </div>

      <div class="pack-meta-row">
        <span class="small">
          {#if selectedPackExists}
            기존 팩 파일: {selectedPackFileName}
          {:else}
            선택한 팩 파일이 없어 신규 팩으로 저장됩니다.
          {/if}
        </span>
        <button class="btn ghost" onclick={() => void refreshPackCatalog()} disabled={isLoadingPackCatalog || isExtracting}>
          {isLoadingPackCatalog ? '스캔 중...' : '폴더 다시 스캔'}
        </button>
      </div>
    </div>

    <div class="extract-panel-footer">
      <button class="btn ghost" onclick={onClose} disabled={isExtracting}>취소</button>
      <button class="btn primary" onclick={handleExtract} disabled={!canExtract || isExtracting}>
        {isExtracting ? '추출 중...' : '추출 실행'}
      </button>
    </div>
  </div>
</div>

<style>
  h2 {
    margin: 0;
    font-size: 15px;
  }

  .small {
    color: #9ca3af;
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }

  .spacer {
    flex: 1;
  }

  .btn {
    background: #1f2635;
    border: 1px solid #0a0d14;
    color: #e5e7eb;
    border-radius: 8px;
    padding: 7px 10px;
    cursor: pointer;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn.primary {
    background: #0a6f87;
    border-color: #064253;
  }

  .btn.ghost {
    background: transparent;
    border-color: #2c3650;
  }

  .lbl-prefix {
    flex-shrink: 0;
  }

  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 90;
  }

  .extract-panel {
    background: #151b2a;
    border: 1px solid #2c3650;
    border-radius: 12px;
    width: min(560px, 92vw);
    max-height: 80vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .extract-panel-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid #2c3650;
  }

  .extract-panel-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px;
    overflow: auto;
  }

  .extract-panel-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid #2c3650;
  }

  .path-hint {
    white-space: normal;
  }

  .full-width {
    width: 100%;
  }

  .clip-name-input {
    background: #1f2635;
    border: 1px solid #2c3650;
    border-radius: 6px;
    color: #e5e7eb;
    padding: 6px 8px;
    font-size: 12px;
    min-width: 0;
    flex: 1;
  }

  .pack-list {
    display: grid;
    gap: 6px;
    border: 1px solid #1f283d;
    border-radius: 8px;
    padding: 8px;
    max-height: 180px;
    overflow: auto;
    background: #0f1320;
  }

  .pack-item {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #e5e7eb;
    font-size: 13px;
  }

  .pack-meta-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 6px;
  }
</style>
