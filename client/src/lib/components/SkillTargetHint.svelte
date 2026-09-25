<script lang="ts">
  interface Props {
    icon: string
    message: string
    onCancel: () => void
  }

  let { icon, message, onCancel }: Props = $props()
</script>

<div class="target-hint" role="status">
  <img src={icon} alt="" />
  <span>{message}</span>
  <button type="button" onclick={onCancel}>Cancel</button>
</div>

<style>
  .target-hint {
    position: fixed;
    z-index: 45;
    bottom: 100px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 10px;
    width: max-content;
    max-width: calc(100vw - 24px);
    box-sizing: border-box;
    padding: 8px 12px;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    background: rgba(6, 10, 14, 0.88);
    backdrop-filter: blur(4px);
    color: #e6edf3;
    font-family: 'Courier New', monospace;
    font-size: 12px;
    pointer-events: auto;
  }
  .target-hint img {
    width: 28px;
    height: 28px;
  }
  .target-hint button {
    cursor: pointer;
    background: rgba(159, 197, 255, 0.08);
    border: 1px solid rgba(159, 197, 255, 0.45);
    border-radius: 4px;
    padding: 4px 8px;
    font-family: inherit;
    color: #9fc5ff;
  }
</style>
