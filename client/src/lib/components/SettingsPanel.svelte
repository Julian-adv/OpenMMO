<script lang="ts">
  import { bgmVolume, bgmMuted } from '../managers/bgmManager'
  import { graphicsQuality, reloadNeeded, type QualityLevel } from '../stores/graphicsSettings'

  interface Props {
    onClose: () => void
  }

  let { onClose }: Props = $props()

  const qualityOptions: { value: QualityLevel; label: string }[] = [
    { value: 'high', label: 'High' },
    { value: 'medium', label: 'Medium' },
    { value: 'low', label: 'Low' },
  ]

  function handleVolumeChange(e: Event) {
    const target = e.target as HTMLInputElement
    bgmVolume.set(parseFloat(target.value))
  }

  function handleVolumeWheel(e: WheelEvent) {
    e.preventDefault()
    const delta = e.deltaY < 0 ? 0.01 : -0.01
    bgmVolume.update((v) => Math.round(Math.max(0, Math.min(1, v + delta)) * 100) / 100)
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onClose}>
  <div class="panel" onclick={(e) => e.stopPropagation()}>
    <div class="header">
      <h2>Settings</h2>
      <button class="close-btn" onclick={onClose}>&times;</button>
    </div>

    <div class="setting-row">
      <span class="setting-label">Graphics Quality</span>
      <div class="quality-row">
        {#each qualityOptions as opt (opt.value)}
          <button
            class="quality-btn"
            class:active={$graphicsQuality === opt.value}
            onclick={() => graphicsQuality.set(opt.value)}
          >
            {opt.label}
          </button>
        {/each}
      </div>
      {#if $reloadNeeded}
        <div class="reload-notice">
          <span>Antialiasing changes require restart</span>
          <button class="reload-btn" onclick={() => location.reload()}>Restart</button>
        </div>
      {/if}
    </div>

    <div class="divider"></div>

    <div class="setting-row">
      <label for="bgm-volume">BGM Volume</label>
      <div class="slider-row">
        <button
          class="mute-btn"
          class:muted={$bgmMuted}
          onclick={() => bgmMuted.update((m) => !m)}
          title={$bgmMuted ? 'Unmute' : 'Mute'}
        >
          {#if $bgmMuted}
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 576 512"><path fill="currentColor" d="M301.1 34.8C312.6 40 320 51.4 320 64v384c0 12.6-7.4 24-18.9 29.2s-25 3.1-34.4-5.3L131.8 352H64c-35.3 0-64-28.7-64-64v-64c0-35.3 28.7-64 64-64h67.8L266.7 40.1c9.4-8.4 22.9-10.4 34.4-5.3zM425 167l55 55 55-55c9.4-9.4 24.6-9.4 33.9 0s9.4 24.6 0 33.9l-55 55 55 55c9.4 9.4 9.4 24.6 0 33.9s-24.6 9.4-33.9 0l-55-55-55 55c-9.4 9.4-24.6 9.4-33.9 0s-9.4-24.6 0-33.9l55-55-55-55c-9.4-9.4-9.4-24.6 0-33.9s24.6-9.4 33.9 0z"/></svg>
          {:else}
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 448 512"><path fill="currentColor" d="M301.1 34.8C312.6 40 320 51.4 320 64v384c0 12.6-7.4 24-18.9 29.2s-25 3.1-34.4-5.3L131.8 352H64c-35.3 0-64-28.7-64-64v-64c0-35.3 28.7-64 64-64h67.8L266.7 40.1c9.4-8.4 22.9-10.4 34.4-5.3zM412.6 181.5C434.1 199.1 448 225.9 448 256s-13.9 56.9-35.4 74.5c-10.3 8.4-25.4 6.8-33.8-3.5s-6.8-25.4 3.5-33.8C393.1 284.4 400 271 400 256s-6.9-28.4-17.7-37.3c-10.3-8.4-11.8-23.5-3.5-33.8s23.5-11.8 33.8-3.5z"/></svg>
          {/if}
        </button>
        <input
          type="range"
          id="bgm-volume"
          min="0"
          max="1"
          step="0.01"
          value={$bgmVolume}
          oninput={handleVolumeChange}
          onwheel={handleVolumeWheel}
          disabled={$bgmMuted}
        />
        <span class="volume-value">{$bgmMuted ? 'MUTE' : `${Math.round($bgmVolume * 100)}%`}</span>
      </div>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 10000;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .panel {
    background: #1a202c;
    border: 1px solid #4a5568;
    border-radius: 10px;
    padding: 24px;
    width: 320px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6);
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 20px;
  }

  h2 {
    margin: 0;
    color: #edf2f7;
    font-size: 18px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .close-btn {
    background: none;
    border: none;
    color: #a0aec0;
    font-size: 24px;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
  }

  .close-btn:hover {
    color: #fff;
  }

  .setting-row {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .divider {
    height: 1px;
    background: #2d3748;
    margin: 16px 0;
  }

  label,
  .setting-label {
    color: #a0aec0;
    font-size: 13px;
    font-weight: 500;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .quality-row {
    display: flex;
    gap: 0;
    border-radius: 6px;
    overflow: hidden;
    border: 1px solid #4a5568;
  }

  .quality-btn {
    flex: 1;
    padding: 7px 0;
    background: #2d3748;
    color: #a0aec0;
    border: none;
    font-size: 13px;
    font-weight: 500;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    cursor: pointer;
    transition: background 150ms ease, color 150ms ease;
  }

  .quality-btn:not(:last-child) {
    border-right: 1px solid #4a5568;
  }

  .quality-btn:hover {
    background: #374151;
    color: #edf2f7;
  }

  .quality-btn.active {
    background: #4299e1;
    color: #fff;
  }

  .reload-notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
    padding: 6px 10px;
    background: #2d3748;
    border: 1px solid #4a5568;
    border-radius: 6px;
    font-size: 12px;
    color: #ecc94b;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .reload-btn {
    background: #4a5568;
    color: #edf2f7;
    border: none;
    border-radius: 4px;
    padding: 3px 10px;
    font-size: 12px;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    cursor: pointer;
    transition: background 150ms ease;
    flex-shrink: 0;
  }

  .reload-btn:hover {
    background: #718096;
  }

  .slider-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .mute-btn {
    background: none;
    border: none;
    color: #a0aec0;
    cursor: pointer;
    padding: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    flex-shrink: 0;
    transition: color 150ms ease;
  }

  .mute-btn:hover {
    color: #fff;
  }

  .mute-btn.muted {
    color: #fc8181;
  }

  input[type='range'] {
    flex: 1;
    accent-color: #4299e1;
    height: 6px;
  }

  input[type='range']:disabled {
    opacity: 0.3;
  }

  .volume-value {
    color: #edf2f7;
    font-size: 13px;
    font-family: 'Courier New', monospace;
    min-width: 36px;
    text-align: right;
  }
</style>
