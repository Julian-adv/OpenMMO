<script lang="ts">
  import './hudDialog.css'
  import { t } from '../i18n'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import { gameStore } from '../stores/gameStore'
  import { travelDistance } from '../utils/autoTravel'
  import { formatDistance } from '../utils/formatDistance'
  import {
    max_bookmark_name_chars,
    max_bookmarks,
  } from '../wasm/onlinerpg_shared'
  import {
    bookmarkDialog,
    bookmarks,
    type Bookmark,
  } from '../stores/bookmarkStore'

  const MAX_BOOKMARKS = max_bookmarks()
  const MAX_NAME_CHARS = max_bookmark_name_chars()

  const scroll = $derived($bookmarkDialog?.scroll ?? null)
  const here = $derived($gameStore.currentPlayer?.position)
  const sorted = $derived(
    [...$bookmarks].sort((a, b) => distance(a) - distance(b))
  )
  let newName = $state('')
  let renaming = $state<{ id: number; name: string } | null>(null)

  const close = () => bookmarkDialog.set(null)

  $effect(() => mountOverlay('bookmarks', close))

  function distance(bookmark: Bookmark) {
    return here ? travelDistance(here, bookmark) : 0
  }

  function save(event: SubmitEvent) {
    event.preventDefault()
    if (!newName.trim()) return
    networkManager.sendSaveBookmark(newName)
    newName = ''
  }

  function rename(event: SubmitEvent) {
    event.preventDefault()
    if (!renaming) return
    networkManager.sendRenameBookmark(renaming.id, renaming.name)
    renaming = null
  }

  function go(bookmark: Bookmark) {
    if (scroll === null) return
    networkManager.sendTeleportToBookmark(scroll, bookmark.id)
    close()
  }

  function goRandom() {
    if (scroll === null) return
    networkManager.sendTeleportScroll(scroll)
    close()
  }
</script>

<div class="hud-dialog-layer">
  <div
    class="hud-dialog bookmark-dialog"
    role="dialog"
    aria-labelledby="bookmark-title"
  >
    <div class="panel-header">
      <h2 class="panel-title" id="bookmark-title">
        {$t(scroll === null ? 'bookmark.title' : 'bookmark.chooseTitle')}
      </h2>
      <span class="count">{$bookmarks.length} / {MAX_BOOKMARKS}</span>
      <button
        type="button"
        class="close-btn"
        onclick={close}
        title={$t('common.close')}
        aria-label={$t('common.close')}>&times;</button
      >
    </div>
    {#if scroll === null}
      <p class="hint">{$t('bookmark.hint')}</p>
    {/if}
    {#if sorted.length === 0}
      <p class="hint">{$t('bookmark.empty')}</p>
    {:else}
      <ul>
        {#each sorted as bookmark (bookmark.id)}
          <li>
            {#if renaming?.id === bookmark.id}
              <form class="row" onsubmit={rename}>
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  bind:value={renaming.name}
                  maxlength={MAX_NAME_CHARS}
                  autofocus
                />
                <button type="submit" class="btn primary"
                  >{$t('common.confirm')}</button
                >
                <button
                  type="button"
                  class="btn"
                  onclick={() => (renaming = null)}
                  >{$t('common.cancel')}</button
                >
              </form>
            {:else}
              <span class="name">{bookmark.name}</span>
              <span class="distance">{formatDistance(distance(bookmark))}</span>
              {#if scroll === null}
                <button
                  type="button"
                  class="btn"
                  onclick={() =>
                    (renaming = { id: bookmark.id, name: bookmark.name })}
                  >{$t('bookmark.rename')}</button
                >
                <button
                  type="button"
                  class="btn"
                  onclick={() => networkManager.sendDeleteBookmark(bookmark.id)}
                  >{$t('bookmark.delete')}</button
                >
              {:else}
                <button
                  type="button"
                  class="btn primary"
                  onclick={() => go(bookmark)}>{$t('bookmark.go')}</button
                >
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if scroll === null}
      <form class="row" onsubmit={save}>
        <input
          bind:value={newName}
          maxlength={MAX_NAME_CHARS}
          placeholder={$t('bookmark.namePlaceholder')}
        />
        <button
          type="submit"
          class="btn primary"
          disabled={!newName.trim() || $bookmarks.length >= MAX_BOOKMARKS}
          >{$t('bookmark.save')}</button
        >
      </form>
    {/if}
    {#if $bookmarkDialog?.error}
      <p class="error" role="alert">
        {$t(`bookmark.error.${$bookmarkDialog.error}`)}
      </p>
    {/if}
    {#if scroll !== null}
      <div class="actions">
        <button type="button" class="btn" onclick={goRandom}
          >{$t('bookmark.random')}</button
        >
      </div>
    {/if}
  </div>
</div>

<style>
  .bookmark-dialog {
    width: 360px;
  }
  .count {
    margin-left: auto;
    color: #9fb2c3;
    font-size: 11px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 45vh;
    overflow-y: auto;
  }
  li,
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  li {
    padding: 4px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }
  .row,
  .row input {
    flex: 1;
  }
  input {
    min-width: 0;
    padding: 4px 6px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.35);
    color: inherit;
    font: inherit;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .distance {
    color: #9fb2c3;
    white-space: nowrap;
  }
</style>
