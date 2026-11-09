<script lang="ts">
  import { tick } from 'svelte'
  import { t, type MessageKey } from '../i18n'
  import { formatCalendarDate } from '../data/gameCalendar'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import {
    closeNoticeBoard,
    noticeBoard,
    NOTICE_BODY_MAX_CHARS,
    NOTICE_COMMENT_MAX_CHARS,
    NOTICE_TITLE_MAX_CHARS,
    type GameDateTime,
    type NoticeBoardSession,
  } from '../stores/noticeBoardStore'

  let { session }: { session: NoticeBoardSession } = $props()

  let dialog = $state<HTMLDivElement>()
  let title = $state('')
  let body = $state('')
  let comment = $state('')

  const view = $derived(session.view)

  $effect(() => mountOverlay('noticeBoard', closeNoticeBoard))
  $effect(() => {
    tick().then(() => dialog?.focus())
  })

  function formatTime(at: GameDateTime) {
    const time = `${String(at.hour).padStart(2, '0')}:${String(at.minute).padStart(2, '0')}`
    return `${formatCalendarDate($t, at)} ${time}`
  }

  function setView(next: NoticeBoardSession['view']) {
    noticeBoard.update((s) => (s ? { ...s, view: next, refusal: null } : s))
  }

  function markPending() {
    noticeBoard.update((s) => (s ? { ...s, pending: true, refusal: null } : s))
  }

  function openPost(postId: number) {
    markPending()
    networkManager.sendReadNoticePost(session.objectId, postId)
  }

  function submitPost(event: SubmitEvent) {
    event.preventDefault()
    if (!title.trim() || !body.trim() || session.pending) return
    markPending()
    networkManager.sendWriteNoticePost(session.objectId, title, body)
  }

  function submitComment(event: Event, postId: number) {
    event.preventDefault()
    if (!comment.trim() || session.pending) return
    markPending()
    networkManager.sendWriteNoticeComment(session.objectId, postId, comment)
  }

  // A reply clears the form that produced it.
  let lastView: NoticeBoardSession['view'] | null = null
  $effect(() => {
    if (lastView && view !== lastView && view.kind === 'post') {
      if (lastView.kind === 'write') {
        title = ''
        body = ''
      } else {
        comment = ''
      }
    }
    lastView = view
  })
</script>

<div class="board-backdrop">
  <div
    class="board-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="board-title"
    tabindex="-1"
    bind:this={dialog}
  >
    <header>
      <h2 id="board-title">{$t('noticeBoard.title')}</h2>
      <div class="header-actions">
        {#if view.kind === 'list'}
          <button class="primary" onclick={() => setView({ kind: 'write' })}
            >{$t('noticeBoard.write')}</button
          >
        {:else}
          <button class="secondary" onclick={() => setView({ kind: 'list' })}
            >{$t('noticeBoard.back')}</button
          >
        {/if}
        <button
          class="secondary"
          aria-label={$t('noticeBoard.close')}
          onclick={closeNoticeBoard}>×</button
        >
      </div>
    </header>

    {#if session.refusal}
      <p class="refusal">
        {$t(`noticeBoard.refused.${session.refusal}` as MessageKey)}
      </p>
    {/if}

    {#if view.kind === 'list'}
      {#if session.posts.length === 0}
        <p class="muted">{$t('noticeBoard.empty')}</p>
      {:else}
        <ul class="posts">
          {#each session.posts as post (post.id)}
            <li>
              <button class="post-row" onclick={() => openPost(post.id)}>
                <span class="post-title">{post.title}</span>
                {#if post.comment_count > 0}
                  <span class="count">[{post.comment_count}]</span>
                {/if}
                <span class="meta"
                  >{post.author} · {formatTime(post.written_at)}</span
                >
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if view.kind === 'write'}
      <form class="write" onsubmit={submitPost}>
        <input
          bind:value={title}
          maxlength={NOTICE_TITLE_MAX_CHARS}
          placeholder={$t('noticeBoard.titlePlaceholder')}
        />
        <textarea
          bind:value={body}
          maxlength={NOTICE_BODY_MAX_CHARS}
          rows="8"
          placeholder={$t('noticeBoard.bodyPlaceholder')}></textarea>
        <div class="row-end">
          <span class="muted">{body.length}/{NOTICE_BODY_MAX_CHARS}</span>
          <button
            class="primary"
            type="submit"
            disabled={session.pending || !title.trim() || !body.trim()}
            >{$t('noticeBoard.submit')}</button
          >
        </div>
      </form>
    {:else}
      <article>
        <h3>{view.post.title}</h3>
        <div class="meta">
          {view.post.author} · {formatTime(view.post.written_at)}
        </div>
        <p class="body">{view.post.body}</p>
      </article>
      <section class="comments">
        <h4>
          {$t('noticeBoard.comments', { count: view.comments.length })}
        </h4>
        {#if view.comments.length === 0}
          <p class="muted">{$t('noticeBoard.noComments')}</p>
        {:else}
          <ul>
            {#each view.comments as c (c.id)}
              <li>
                <span class="author">{c.author}</span>
                <span class="comment-body">{c.body}</span>
                <span class="meta">{formatTime(c.written_at)}</span>
              </li>
            {/each}
          </ul>
        {/if}
        <form
          class="comment-form"
          onsubmit={(event) => submitComment(event, view.post.id)}
        >
          <textarea
            bind:value={comment}
            maxlength={NOTICE_COMMENT_MAX_CHARS}
            rows="2"
            placeholder={$t('noticeBoard.commentPlaceholder')}></textarea>
          <button
            class="primary"
            type="submit"
            disabled={session.pending || !comment.trim()}
            >{$t('noticeBoard.commentSubmit')}</button
          >
        </form>
      </section>
    {/if}
  </div>
</div>

<style>
  .board-backdrop {
    position: absolute;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    padding: 16px;
    box-sizing: border-box;
    background: rgba(8, 5, 4, 0.45);
  }
  .board-dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: min(520px, calc(100vw - 32px));
    max-height: min(640px, calc(100vh - 32px));
    box-sizing: border-box;
    padding: 18px 20px;
    overflow-y: auto;
    border: 1px solid #6b5338;
    border-radius: 10px;
    outline: none;
    background: #1d1711;
    color: #efe3cc;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.55);
    font-family: 'Noto Sans KR', sans-serif;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .header-actions {
    display: flex;
    gap: 6px;
  }
  h2 {
    margin: 0;
    color: #f6e6c4;
    font-size: 18px;
  }
  h3 {
    margin: 0 0 4px;
    font-size: 16px;
    word-break: break-word;
  }
  h4 {
    margin: 0 0 8px;
    color: #cdb98f;
    font-size: 13px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .posts li + li {
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .post-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px 8px;
    width: 100%;
    padding: 8px 4px;
    border: none;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .post-row:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .post-title {
    overflow: hidden;
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: #e2b05a;
    font-size: 12px;
  }
  .meta {
    grid-column: 1 / -1;
    color: #998a73;
    font-size: 11px;
  }
  .muted {
    margin: 0;
    color: #998a73;
    font-size: 12px;
  }
  .refusal {
    margin: 0;
    padding: 8px 10px;
    border-left: 3px solid #b95749;
    background: rgba(145, 55, 44, 0.16);
    color: #e3c7bd;
    font-size: 12px;
  }
  .body {
    margin: 10px 0 0;
    font-size: 13px;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .comments {
    padding-top: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
  }
  .comments li {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: baseline;
    padding: 4px 0;
    font-size: 12px;
  }
  .author {
    color: #e2c48a;
    font-weight: 700;
  }
  .comment-body {
    flex: 1;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .comments .meta {
    font-size: 10px;
  }
  form.write {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .comment-form {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    margin-top: 8px;
  }
  .comment-form textarea {
    flex: 1;
  }
  input,
  textarea {
    box-sizing: border-box;
    padding: 8px 10px;
    border: 1px solid #5a4835;
    border-radius: 6px;
    background: #120e0a;
    color: inherit;
    font: inherit;
    font-size: 13px;
    resize: vertical;
  }
  .row-end {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  button.primary,
  button.secondary {
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
  button.primary {
    border: 1px solid #c58a3a;
    background: #7a4e1c;
    color: #fff3df;
  }
  button.secondary {
    border: 1px solid #675b4e;
    background: #2b251f;
    color: #ddd1bf;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button:not(:disabled):hover {
    filter: brightness(1.12);
  }
</style>
