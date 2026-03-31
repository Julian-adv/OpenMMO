<script lang="ts">
  import { gameStore } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { handleCommand } from '../chat-commands'

  type Tab = 'say' | 'combat'

  let activeTab = $state<Tab>('say')
  let chatMessages = $derived($gameStore.chatMessages)
  let combatMessages = $derived($gameStore.combatMessages)
  let isConnected = $derived($gameStore.isConnected)
  let messageInput = $state('')
  let chatContainer = $state<HTMLDivElement>()

  $effect(() => {
    const len =
      activeTab === 'say' ? chatMessages.length : combatMessages.length
    if (chatContainer && len) {
      chatContainer.scrollTop = chatContainer.scrollHeight
    }
  })

  function sendMessage() {
    const trimmed = messageInput.trim()
    if (!trimmed) return
    if (handleCommand(trimmed)) {
      messageInput = ''
      return
    }
    if (isConnected) {
      networkManager.sendChatMessage(trimmed)
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
      activeTab = 'say'
      chatInput?.focus()
    }
  }

  let chatInput = $state<HTMLInputElement>()
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="chat-panel">
  <div class="tabs">
    <button
      class="tab"
      class:active={activeTab === 'say'}
      onclick={() => (activeTab = 'say')}
    >
      Say
    </button>
    <button
      class="tab"
      class:active={activeTab === 'combat'}
      onclick={() => (activeTab = 'combat')}
    >
      Combat
    </button>
  </div>

  <div class="chat-messages" bind:this={chatContainer}>
    {#if activeTab === 'say'}
      {#each chatMessages as entry, index (index)}
        <div class="message">
          {#if entry.name}
            <span class="name" class:local={entry.sender === 'local'} class:remote={entry.sender === 'remote'}>{entry.name}:</span>
            {entry.text}
          {:else}
            <span class="system">{entry.text}</span>
          {/if}
        </div>
      {/each}
    {:else}
      {#each combatMessages as entry, index (index)}
        <div class="message combat">
          {#if entry.name}
            <span class="name" class:local={entry.sender === 'local'} class:remote={entry.sender === 'remote'}>{entry.name}:</span>
            <span class:hit={entry.hit === true} class:miss={entry.hit === false}>{entry.text}</span>
          {:else}
            {entry.text}
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  {#if activeTab === 'say'}
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
  {/if}
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

  .tabs {
    display: flex;
    border-bottom: 1px solid #4a5568;
    flex-shrink: 0;
  }

  .tab {
    flex: 1;
    padding: 6px 0;
    border: none;
    background: transparent;
    color: #718096;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: color 0.15s, background 0.15s;
    border-radius: 8px 8px 0 0;
  }

  .tab:hover {
    color: #e2e8f0;
  }

  .tab.active {
    color: #e2e8f0;
    background: rgba(255, 255, 255, 0.05);
    border-bottom: 2px solid #4299e1;
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
    overflow-wrap: break-word;
    text-align: left;
    max-width: 100%;
  }

  .name.local {
    color: #68d391;
    font-weight: 600;
  }

  .name.remote {
    color: #f6e05e;
    font-weight: 600;
  }

  .system {
    color: #a0aec0;
    font-style: italic;
  }

  .message.combat {
    color: #f6ad55;
  }

  .hit {
    color: #68d391;
  }

  .miss {
    color: #fc8181;
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
