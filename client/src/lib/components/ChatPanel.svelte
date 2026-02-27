<script lang="ts">
  import { gameStore } from '../stores/gameStore'
  import { networkManager } from '../network/socket'

  let chatMessages = $derived($gameStore.chatMessages)
  let isConnected = $derived($gameStore.isConnected)
  let messageInput = $state('')
  let chatContainer: HTMLDivElement

  $effect(() => {
    // Auto-scroll to bottom when new messages arrive
    if (chatContainer && chatMessages.length) {
      chatContainer.scrollTop = chatContainer.scrollHeight
    }
  })

  function sendMessage() {
    if (messageInput.trim() && isConnected) {
      networkManager.sendChatMessage(messageInput.trim())
      messageInput = ''
    }
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault()
      sendMessage()
    }
  }

  function handleGlobalKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && document.activeElement !== chatInput) {
      event.preventDefault()
      chatInput?.focus()
    }
  }

  let chatInput: HTMLInputElement
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="chat-panel">
  <div class="chat-messages" bind:this={chatContainer}>
    {#each chatMessages as message, index (index)}
      <div class="message">
        {message}
      </div>
    {/each}
  </div>

  <div class="chat-input" class:disconnected={!isConnected}>
    <input
      type="text"
      bind:this={chatInput}
      bind:value={messageInput}
      onkeydown={handleKeyDown}
      placeholder="Type a message..."
      disabled={!isConnected}
    />
    <button
      onclick={sendMessage}
      disabled={!isConnected || !messageInput.trim()}
    >
      Send
    </button>
  </div>
</div>

<style>
  .chat-panel {
    position: fixed;
    bottom: 20px;
    left: 20px;
    width: 350px;
    height: 300px;
    background: rgba(0, 0, 0, 0.8);
    border: 1px solid #4a5568;
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .chat-messages {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 10px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 5px;
    width: 100%;
    box-sizing: border-box;
  }

  .message {
    color: #e2e8f0;
    font-size: 12px;
    line-height: 1.4;
    word-wrap: break-word;
    word-break: break-all;
    text-align: left;
    max-width: 100%;
  }

  .chat-input {
    display: flex;
    gap: 8px;
    border-top: 1px solid #4a5568;
    background: #1a202c;
    border-radius: 0 0 8px 8px;
  }

  .chat-input.disconnected {
    background: #742a2a;
  }

  .chat-input input {
    flex: 1;
    padding: 8px 10px;
    border: none;
    border-radius: 0 0 0 8px;
    background: transparent;
    color: #ffffff;
    font-size: 12px;
  }

  .chat-input input:focus {
    outline: none;
    border-color: #4299e1;
  }

  .chat-input input:disabled {
    opacity: 0.5;
  }

  .chat-input button {
    margin: 2px;
    padding: 8px 15px;
    border: none;
    border-radius: 4px;
    background: #4299e1;
    color: #ffffff;
    font-size: 12px;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .chat-input button:hover:not(:disabled) {
    background: #3182ce;
  }

  .chat-input button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .chat-messages::-webkit-scrollbar {
    width: 6px;
  }

  .chat-messages::-webkit-scrollbar-track {
    background: #2d3748;
  }

  .chat-messages::-webkit-scrollbar-thumb {
    background: #4a5568;
    border-radius: 3px;
  }
</style>
