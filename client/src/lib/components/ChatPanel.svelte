<script lang="ts">
  import { gameStore } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { handleCommand, commandNames } from '../chat-commands'
  import { chatFocusRequest } from '../stores/npcMenuStore'

  type Tab = 'say' | 'combat'
  const TRANSCRIPT_FADE_DELAY_MS = 10_000

  let activeTab = $state<Tab>('say')
  let chatMessages = $derived($gameStore.chatMessages)
  let combatMessages = $derived($gameStore.combatMessages)
  let isConnected = $derived($gameStore.isConnected)
  let messageInput = $state('')
  let chatContainer = $state<HTMLDivElement>()
  let transcriptVisible = $state(true)
  let fadeTimer: number | undefined

  $effect(() => {
    const len =
      activeTab === 'say' ? chatMessages.length : combatMessages.length
    if (chatContainer && len) {
      chatContainer.scrollTop = chatContainer.scrollHeight
    }
  })

  function revealTranscript() {
    transcriptVisible = true
    window.clearTimeout(fadeTimer)
    fadeTimer = window.setTimeout(() => {
      transcriptVisible = false
    }, TRANSCRIPT_FADE_DELAY_MS)
  }

  // Re-reveal the transcript on new chat/combat activity. Tab switches and
  // input focus reveal it directly via setActiveTab().
  $effect(() => {
    void chatMessages.length
    void combatMessages.length
    revealTranscript()
    return () => window.clearTimeout(fadeTimer)
  })

  function setActiveTab(tab: Tab) {
    activeTab = tab
    revealTranscript()
  }

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

  let tabCycle: { matches: string[]; index: number } | null = null

  function completeCommand() {
    if (!messageInput.startsWith('/') || messageInput.includes(' ')) return
    if (tabCycle && tabCycle.matches[tabCycle.index] === messageInput) {
      tabCycle.index = (tabCycle.index + 1) % tabCycle.matches.length
      messageInput = tabCycle.matches[tabCycle.index]
      return
    }
    const matches = commandNames.filter((n) => n.startsWith(messageInput))
    if (matches.length === 0) return
    tabCycle = { matches, index: 0 }
    messageInput = matches[0]
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Tab') {
      event.preventDefault()
      completeCommand()
      return
    }
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

  function restoreViewportAfterKeyboard() {
    for (const delay of [0, 80, 250]) {
      window.setTimeout(() => {
        window.scrollTo(0, 0)
        document.documentElement.scrollTop = 0
        document.body.scrollTop = 0
      }, delay)
    }
  }

  let chatInput = $state<HTMLInputElement>()

  // NPC "Talk" action (click or context menu) asks for input focus.
  $effect(() => {
    if ($chatFocusRequest > 0) {
      activeTab = 'say'
      chatInput?.focus()
    }
  })
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<div class="chat-panel" class:transcript-faded={!transcriptVisible}>
  <div class="tabs">
    <button
      class="tab"
      class:active={activeTab === 'say'}
      onclick={() => setActiveTab('say')}
    >
      Chat
    </button>
    <button
      class="tab"
      class:active={activeTab === 'combat'}
      onclick={() => setActiveTab('combat')}
    >
      Combat
    </button>
  </div>

  <div class="chat-messages" bind:this={chatContainer}>
    {#if activeTab === 'say'}
      {#each chatMessages as entry, index (index)}
        <div class="message">
          {#if entry.name}
            <span
              class="name"
              class:local={entry.sender === 'local'}
              class:remote={entry.sender === 'remote'}>{entry.name}:</span
            >
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
            <span
              class="name"
              class:local={entry.sender === 'local'}
              class:remote={entry.sender === 'remote'}>{entry.name}:</span
            >
            <span
              class:hit={entry.hit === true}
              class:miss={entry.hit === false}>{entry.text}</span
            >
          {:else}
            {entry.text}
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  <div class="chat-input" class:disconnected={!isConnected}>
    <input
      type="text"
      bind:this={chatInput}
      bind:value={messageInput}
      onkeydown={handleKeyDown}
      onfocus={() => setActiveTab('say')}
      onblur={restoreViewportAfterKeyboard}
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
    bottom: 9px;
    left: 9px;
    width: 350px;
    height: 300px;
    background: rgba(0, 0, 0, 0.8);
    border: 1px solid #4a5568;
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    transition:
      background 700ms ease,
      border-color 700ms ease;
  }

  .chat-panel.transcript-faded {
    background: transparent;
    border-color: transparent;
    pointer-events: none;
  }

  .chat-panel.transcript-faded .chat-input,
  .chat-panel.transcript-faded .chat-input * {
    pointer-events: auto;
  }

  .tabs {
    display: flex;
    border-bottom: 1px solid #4a5568;
    flex-shrink: 0;
    transition: opacity 700ms ease;
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
    transition:
      color 0.15s,
      background 0.15s;
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
    transition: opacity 700ms ease;
  }

  .chat-panel.transcript-faded .tabs,
  .chat-panel.transcript-faded .chat-messages {
    opacity: 0;
    pointer-events: none;
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

  .chat-input input::placeholder {
    color: rgba(113, 128, 150, 0.5);
    font-size: 11px;
    opacity: 1;
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

  /* Narrow: the quickslot bar wraps to two rows and is half as wide (~226px),
     so the chat stays at the bottom-left (base position) instead of being lifted
     above the bar. Cap the width so its right edge clears the centred bar.
     (These constants mirror the bar/menu dimensions owned by QuickslotBar and
     GameHud; if those change, re-derive the caps here.) */
  @media (max-width: 1200px) {
    .chat-panel {
      width: min(350px, calc(50vw - 130px));
    }
  }

  /* Desktop-narrow: the quickslot bar is tucked to the right against the menu
     buttons, so the chat fills the left side. Cap clears the bar's left edge:
     387 = menu(124) + 9 screen-margin + 16 gap + bar(217) + 12 gap + 9 left. */
  @media (min-width: 601px) and (max-width: 999.98px) and (pointer: fine) {
    .chat-panel {
      width: min(450px, calc(100vw - 387px));
    }
  }

  /* 1000–1200px: the bar is a right-aligned single row (~444px) reaching further
     left, so the chat gets less room: 702 = menu(212) + 9 + 16 + bar(444) + 12 + 9. */
  @media (min-width: 1000px) and (max-width: 1200px) {
    .chat-panel {
      width: min(450px, calc(100vw - 702px));
    }
  }

  @media (max-width: 600px), (pointer: coarse) and (max-width: 900px) {
    .chat-panel {
      left: max(9px, env(safe-area-inset-left));
      bottom: max(9px, env(safe-area-inset-bottom));
      /* Clear the centred two-row quickslot bar (gets quite narrow on phones). */
      width: min(300px, calc(50vw - 120px));
      height: min(124px, 22dvh);
      box-sizing: border-box;
      border-radius: 6px;
    }

    .tabs {
      position: absolute;
      top: 0;
      left: calc(100% + 4px);
      display: flex;
      flex-direction: column;
      gap: 4px;
      border-bottom: none;
    }

    .tab {
      flex: none;
      width: 46px;
      height: 24px;
      padding: 0;
      border: 1px solid #4a5568;
      border-radius: 5px;
      background: rgba(26, 32, 44, 0.88);
      font-size: 9px;
      line-height: 1;
    }

    .tab.active {
      background: rgba(66, 153, 225, 0.26);
      border-color: rgba(66, 153, 225, 0.7);
      border-bottom: 1px solid rgba(66, 153, 225, 0.7);
    }

    .chat-messages {
      padding: 5px 6px;
      gap: 2px;
    }

    .message {
      font-size: 10px;
      line-height: 1.25;
    }

    .chat-input {
      gap: 4px;
      border-radius: 0 0 6px 6px;
    }

    .chat-input input {
      padding: 4px 6px;
      font-size: 16px;
      line-height: 1;
      border-radius: 0 0 0 6px;
      min-width: 0;
    }

    .chat-input input::placeholder {
      font-size: 12px;
    }

    .chat-input button {
      margin: 2px;
      padding: 4px 8px;
      font-size: 11px;
    }
  }

  @media (orientation: landscape) and (pointer: coarse) and (max-height: 600px) {
    .chat-panel {
      bottom: 2px;
    }
  }
</style>
