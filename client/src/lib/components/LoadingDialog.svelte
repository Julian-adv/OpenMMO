<script lang="ts">
  interface Props {
    message?: string
  }

  let { message = 'Loading...' }: Props = $props()
</script>

<div class="loading-backdrop">
  <div class="loading-dialog" role="dialog" aria-modal="true">
    <div class="spinner"></div>
    <p>{message}</p>
  </div>
</div>

<style>
  .loading-backdrop {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.45);
    z-index: 30;
  }

  .loading-dialog {
    width: min(380px, calc(100vw - 32px));
    padding: 20px;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(16, 16, 16, 0.95);
    color: #f4f4f4;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
  }

  .loading-dialog p {
    margin: 0;
    font-size: 18px;
    color: #d4d4d4;
  }

  .spinner {
    width: 36px;
    height: 36px;
    border: 3px solid rgba(255, 255, 255, 0.15);
    border-top-color: #e2b93b;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
