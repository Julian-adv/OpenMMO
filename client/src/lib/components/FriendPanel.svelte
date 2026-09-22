<script lang="ts">
  import { t } from '../i18n'
  import {
    friendList,
    friendPanelVisible,
    onlineFriends,
    sortFriends,
    FRIENDS_POLL_OPEN_MS,
    FRIENDS_POLL_CLOSED_MS,
    MAX_FRIENDS,
    type FriendEntry,
  } from '../stores/friendStore'
  import { networkManager } from '../network/socket'
  import { requestChatDraft } from '../stores/npcMenuStore'
  import { draggablePanel } from '../actions/draggablePanel'
  import { classIconPath } from '../stores/partyStore'

  const visible = $derived($friendPanelVisible)
  const friends = $derived(sortFriends($friendList, $onlineFriends))
  const isOnline = (friend: FriendEntry) =>
    $onlineFriends.has(friend.characterId)
  // The live level for online friends; the login snapshot for the rest.
  const levelOf = (friend: FriendEntry) =>
    $onlineFriends.get(friend.characterId) ?? friend.level

  // Poll more often while open; skip empty rosters.
  $effect(() => {
    if ($friendList.length === 0) return
    const period = visible ? FRIENDS_POLL_OPEN_MS : FRIENDS_POLL_CLOSED_MS
    networkManager.sendRequestFriendsOnline()
    const timer = setInterval(
      () => networkManager.sendRequestFriendsOnline(),
      period
    )
    return () => clearInterval(timer)
  })

  function whisper(friend: FriendEntry) {
    requestChatDraft(`/w ${friend.name} `)
  }

  let adding = $state(false)
  let addName = $state('')
  const full = $derived(friends.length >= MAX_FRIENDS)

  function submitAdd() {
    const name = addName.trim()
    if (name) networkManager.sendChatMessage(`/friend add ${name}`)
    addName = ''
    adding = false
  }

  function onAddKeydown(e: KeyboardEvent) {
    e.stopPropagation()
    if (e.key === 'Enter') submitAdd()
    else if (e.key === 'Escape') {
      addName = ''
      adding = false
    }
  }
</script>

{#if visible}
  <div
    class="friend-panel"
    aria-label={$t('friends.title')}
    use:draggablePanel={'friends'}
  >
    <div class="panel-header" data-drag-handle>
      <span class="panel-title">{$t('friends.title')}</span>
      <span class="friend-count">{friends.length}/{MAX_FRIENDS}</span>
      <button
        class="close-btn"
        title={$t('common.close')}
        aria-label={$t('common.close')}
        onclick={() => friendPanelVisible.set(false)}>×</button
      >
    </div>

    {#if friends.length === 0}
      <div class="empty">{$t('friends.empty')}</div>
    {:else}
      <div class="friend-rows">
        {#each friends as friend (friend.characterId)}
          <div class="friend-row" class:offline={!isOnline(friend)}>
            <span
              class="status-dot"
              class:on={isOnline(friend)}
              title={isOnline(friend)
                ? $t('friends.online')
                : $t('friends.offline')}
            ></span>
            {#if classIconPath(friend.class)}
              <img
                class="class-icon"
                src={classIconPath(friend.class)}
                alt={$t(`class.${friend.class}`)}
                title={$t(`class.${friend.class}`)}
                width="14"
                height="14"
              />
            {/if}
            <span class="friend-name">{friend.name}</span>
            <span class="friend-level"
              >{$t('stat.level')} {levelOf(friend)}</span
            >
            <span class="row-actions">
              <button
                class="row-btn"
                title={$t('friends.whisper')}
                aria-label={$t('friends.whisper')}
                disabled={!isOnline(friend)}
                onclick={() => whisper(friend)}
                >{$t('friends.whisperShort')}</button
              >
              <button
                class="row-btn"
                title={$t('friends.inviteParty')}
                aria-label={$t('friends.inviteParty')}
                disabled={!isOnline(friend)}
                onclick={() => networkManager.sendPartyInvite(friend.name)}
                >{$t('friends.invitePartyShort')}</button
              >
              <button
                class="row-btn danger"
                title={$t('friends.remove')}
                aria-label={$t('friends.remove')}
                onclick={() => networkManager.sendFriendRemove(friend.name)}
                >×</button
              >
            </span>
          </div>
        {/each}
      </div>
    {/if}

    {#if adding}
      <div class="add-row">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="add-input"
          placeholder={$t('friends.playerName')}
          aria-label={$t('friends.playerName')}
          maxlength="32"
          autofocus
          bind:value={addName}
          onkeydown={onAddKeydown}
        />
        <button
          class="row-btn"
          title={$t('friends.sendRequest')}
          aria-label={$t('friends.sendRequest')}
          onclick={submitAdd}>✓</button
        >
      </div>
    {:else}
      <div class="add-hint">
        <button class="add-btn" disabled={full} onclick={() => (adding = true)}
          >+ {$t('friends.add')}</button
        >
        <span class="hint">{$t('friends.addHint')}</span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .friend-panel {
    position: fixed;
    right: 16px;
    top: 45%;
    transform: translateY(-50%);
    z-index: 40;
    width: 244px;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    backdrop-filter: blur(4px);
    padding: 10px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 10px;
    background: rgba(6, 10, 14, 0.88);
    color: #e6edf3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 12px;
    pointer-events: auto;
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.15);
    margin-bottom: 8px;
  }

  .panel-title {
    flex: 1;
    font-size: 14px;
    font-weight: 700;
    color: #8fe08f;
  }

  .friend-count {
    color: #7f8f9f;
    font-size: 10px;
  }

  .close-btn {
    background: none;
    border: none;
    color: #9fb2c3;
    font-size: 18px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
  }

  .close-btn:hover {
    color: #fff;
  }

  .add-btn {
    flex-shrink: 0;
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 3px;
    color: #8fe08f;
    font-family: inherit;
    font-size: 11px;
    line-height: 1;
    padding: 3px 6px;
    cursor: pointer;
  }

  .add-btn:hover:not(:disabled) {
    border-color: rgba(143, 224, 143, 0.6);
  }

  .add-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .add-row,
  .add-hint {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
  }

  .add-hint {
    flex-wrap: wrap;
  }

  .add-input {
    flex: 1;
    min-width: 0;
    padding: 2px 6px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 3px;
    color: #e6edf3;
    font-family: inherit;
    font-size: 12px;
    outline: none;
  }

  .add-input:focus {
    border-color: rgba(143, 224, 143, 0.6);
  }

  .empty {
    padding: 6px 2px;
    color: #9fb2c3;
    line-height: 1.6;
  }

  .hint {
    color: #7f8f9f;
  }

  .friend-rows {
    overflow-y: auto;
  }

  .friend-row {
    display: flex;
    align-items: center;
    gap: 6px;
    line-height: 1.9;
    white-space: nowrap;
  }

  .friend-row.offline {
    color: #7f8f9f;
  }

  .status-dot {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: #46525e;
  }

  .status-dot.on {
    background: #8fe08f;
  }

  .class-icon {
    flex: none;
  }

  .friend-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .friend-level {
    color: #9fb2c3;
    font-size: 10px;
  }

  .row-actions {
    display: flex;
    gap: 2px;
  }

  .row-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 3px;
    color: #9fb2c3;
    font-family: inherit;
    font-size: 10px;
    line-height: 1;
    cursor: pointer;
  }

  .row-btn:hover:not(:disabled) {
    color: #fff;
    border-color: rgba(255, 255, 255, 0.4);
  }

  .row-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .row-btn.danger:hover:not(:disabled) {
    color: #ff8f8f;
    border-color: rgba(255, 143, 143, 0.5);
  }
</style>
