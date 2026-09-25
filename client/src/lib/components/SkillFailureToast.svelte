<script lang="ts">
  import { onDestroy } from 'svelte'
  import {
    skillFailure,
    clearSkillFailure,
    SKILL_FAILURE_DURATION_MS,
  } from '../stores/skillFailureStore'

  onDestroy(clearSkillFailure)
</script>

<div
  class="skill-failure-toast"
  role="status"
  aria-atomic="true"
  style:pointer-events="none"
>
  {#if $skillFailure}
    {#key $skillFailure.id}
      <p
        style:animation-duration="{SKILL_FAILURE_DURATION_MS}ms"
        style:pointer-events="none"
      >
        {$skillFailure.text}
      </p>
    {/key}
  {/if}
</div>

<style>
  .skill-failure-toast {
    position: fixed;
    top: 60%;
    left: 50%;
    z-index: 50;
    transform: translate(-50%, -50%);
    width: max-content;
    max-width: min(640px, calc(100vw - 32px));
    color: #ff6b6b;
    font-family: 'Courier New', monospace;
    font-size: 14px;
    line-height: 1.4;
    text-align: center;
    overflow-wrap: anywhere;
    user-select: none;
  }

  p {
    margin: 0;
    padding: 10px 18px;
    border: 1px solid rgba(255, 196, 92, 0.65);
    border-radius: 8px;
    background: rgba(24, 16, 8, 0.94);
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.55);
    animation: skill-failure-fade linear both;
  }

  @keyframes skill-failure-fade {
    0%,
    100% {
      opacity: 0;
    }
    8%,
    80% {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    p {
      animation: none;
    }
  }
</style>
