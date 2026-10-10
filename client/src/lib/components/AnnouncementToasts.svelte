<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t } from '../i18n'
  import {
    announcements,
    clearAnnouncements,
    dismissAnnouncement,
  } from '../stores/announcementStore'

  onDestroy(clearAnnouncements)
</script>

<div class="announcements" role="status" aria-relevant="additions">
  {#each $announcements as announcement (announcement.id)}
    <div class="announcement">
      <div class="heading">
        <strong>{$t('announcements.title')}</strong>
        <button
          type="button"
          aria-label={$t('common.close')}
          onclick={() => dismissAnnouncement(announcement.id)}>×</button
        >
      </div>
      <p>{announcement.text}</p>
    </div>
  {/each}
</div>

<style>
  .announcements {
    position: fixed;
    top: clamp(88px, 16vh, 160px);
    left: 50%;
    z-index: 1101;
    transform: translateX(-50%);
    display: grid;
    gap: 10px;
    width: min(560px, calc(100vw - 32px));
    pointer-events: none;
  }

  .announcement {
    padding: 14px 18px;
    border: 1px solid rgba(255, 196, 92, 0.65);
    border-radius: 8px;
    background: rgba(24, 16, 8, 0.94);
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.55);
    color: #fff2d6;
    line-height: 1.5;
    pointer-events: auto;
    animation: appear 180ms ease-out;
  }

  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: #ffd080;
    font-size: 13px;
  }

  button {
    padding: 0 6px;
    border: 0;
    background: transparent;
    color: inherit;
    font-size: 22px;
    cursor: pointer;
  }

  button:hover {
    color: white;
  }

  p {
    margin: 6px 0 0;
    font-size: 16px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  @keyframes appear {
    from {
      opacity: 0;
      transform: translateY(-8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .announcement {
      animation: none;
    }
  }
</style>
