<script lang="ts">
  interface Props {
    metaText: string
    isLoading: boolean
    hasCandidate: boolean
    hasCandidates: boolean
    hasMainClip: boolean
    hasMergedUnsaved: boolean
    autoRotate: boolean
    loop: boolean
    onMainFileChange: (event: Event) => void
    onExportSelected: () => void
    onExportAll: () => void
    onExtract: () => void
    onStandardizeBones: () => void
    onSave: () => void
    onReset: () => void
    onAutoRotateChange: (value: boolean) => void
    onLoopChange: (value: boolean) => void
  }

  let {
    metaText,
    isLoading,
    hasCandidate,
    hasCandidates,
    hasMainClip,
    hasMergedUnsaved,
    autoRotate,
    loop,
    onMainFileChange,
    onExportSelected,
    onExportAll,
    onExtract,
    onStandardizeBones,
    onSave,
    onReset,
    onAutoRotateChange,
    onLoopChange,
  }: Props = $props()
</script>

<header>
  <h1>GLB Editor</h1>
  <div class="toolbar">
    <label class="btn file">
      메인 GLB 열기
      <input type="file" accept=".glb,.gltf" onchange={onMainFileChange} />
    </label>
    <button class="btn primary" onclick={onExportSelected} disabled={!hasCandidate}>선택 내보내기</button>
    <button class="btn" onclick={onExportAll} disabled={!hasCandidates}>전체 내보내기</button>
    <button class="btn" onclick={onExtract} disabled={!hasMainClip}>애니메이션 추출</button>
    <button class="btn" onclick={onStandardizeBones} disabled={!hasCandidates}>본 이름 표준화</button>
    <button class="btn save" onclick={onSave} disabled={!hasMergedUnsaved}>저장 (파일시스템)</button>
    <button class="btn ghost" onclick={onReset}>초기화</button>
    <span class="small">{isLoading ? '로딩 중...' : metaText}</span>
  </div>
  <div class="spacer"></div>
  <div class="toolbar">
    <label><input type="checkbox" checked={autoRotate} onchange={() => onAutoRotateChange(!autoRotate)} /> AutoRotate</label>
    <label><input type="checkbox" checked={loop} onchange={() => onLoopChange(!loop)} /> Loop</label>
  </div>
</header>

<style>
  header {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    background: #0b0f1a;
    border-bottom: 1px solid #000;
  }

  h1 {
    margin: 0;
    font-size: 15px;
    color: #c7d2fe;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
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

  .btn.save {
    background: #0a7a3e;
    border-color: #065226;
  }

  .file {
    position: relative;
    overflow: hidden;
  }

  .file input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .small {
    color: #9ca3af;
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
</style>
