<script lang="ts">
  import { MIXAMO_BONE_NAMES, type MixamoDetectionResult } from '../mixamo-bones'

  interface Props {
    boneDetection: MixamoDetectionResult
    onApply: (mapping: Record<string, string>) => void
    onCancel: () => void
  }

  let { boneDetection, onApply, onCancel }: Props = $props()

  let manualMapping = $state<Record<string, string>>({ ...boneDetection.nameMap })

  const allBoneNames = $derived([
    ...Object.keys(boneDetection.nameMap),
    ...boneDetection.unmatchedBones,
  ])
  const assignedMixamoNames = $derived(new Set(Object.values(manualMapping)))

  function onManualMappingChange(boneName: string, mixamoName: string): void {
    if (mixamoName === '') {
      const next = { ...manualMapping }
      delete next[boneName]
      manualMapping = next
    } else {
      manualMapping = { ...manualMapping, [boneName]: mixamoName }
    }
  }
</script>

<div class="bone-overlay">
  <div class="bone-panel">
    <div class="bone-panel-header">
      <h2>본 이름 매핑</h2>
      <span class="small">
        자동 매칭: {Object.keys(boneDetection.nameMap).length}개 /
        매칭 안 됨: {boneDetection.unmatchedBones.length}개
      </span>
      <div class="spacer"></div>
      <button class="btn primary" onclick={() => onApply(manualMapping)}>적용</button>
      <button class="btn ghost" onclick={onCancel}>취소</button>
    </div>

    <div class="bone-lists">
      <div class="bone-rows">
        {#each allBoneNames as bone (bone)}
          {@const currentValue = manualMapping[bone] ?? ''}
          {@const isAutoMatched = bone in boneDetection.nameMap}
          <div class="bone-row" class:unmatched={!isAutoMatched}>
            <span class="bone-name">{bone}</span>
            <span class="bone-arrow">→</span>
            <select
              class="bone-select"
              value={currentValue}
              onchange={(e) => onManualMappingChange(bone, (e.currentTarget as HTMLSelectElement).value)}
            >
              <option value="">(매핑 안 함)</option>
              {#if currentValue}
                <option value={currentValue}>{currentValue}</option>
              {/if}
              {#each MIXAMO_BONE_NAMES as mx (mx)}
                {#if !assignedMixamoNames.has(mx) || mx === currentValue}
                  {#if mx !== currentValue}
                    <option value={mx}>{mx}</option>
                  {/if}
                {/if}
              {/each}
            </select>
          </div>
        {/each}
      </div>
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

  .bone-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .bone-panel {
    background: #151b2a;
    border: 1px solid #2c3650;
    border-radius: 12px;
    width: min(700px, 90vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .bone-panel-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid #2c3650;
    flex-wrap: wrap;
  }

  .bone-lists {
    overflow: auto;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .bone-rows {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .bone-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 12px;
  }

  .bone-row.unmatched {
    background: #1a2236;
  }

  .bone-name {
    flex: 1;
    overflow-wrap: anywhere;
    color: #e5e7eb;
  }

  .bone-arrow {
    color: #4b5563;
    flex-shrink: 0;
  }

  .bone-select {
    flex: 1;
    background: #1f2635;
    border: 1px solid #2c3650;
    border-radius: 4px;
    color: #e5e7eb;
    padding: 3px 6px;
    font-size: 12px;
  }
</style>
