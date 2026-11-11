<script lang="ts">
  import { tick } from 'svelte'
  import { t, type MessageKey } from '../i18n'
  import { formatGameDateTime } from '../data/gameCalendar'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import { capeTextureUrl, uploadCapeTexture } from '../utils/networkUtils'
  import {
    BOARD_FACE_H,
    BOARD_FACE_W,
    clampPaperPosition,
    closeNoticeBoard,
    freePaperSpot,
    noticeBoard,
    noticePaperCount,
    NOTICE_BODY_MAX_CHARS,
    NOTICE_COMMENT_MAX_CHARS,
    NOTICE_POSTS_PER_WRITER,
    NOTICE_TITLE_MAX_CHARS,
    PAPER_H,
    PAPER_W,
    paperTilt,
    PAPER_SCALE,
    spotAllowed,
    type GameDateTime,
    noticeExcerpt,
    type NoticeBoardRefusal,
    type NoticeBoardSession,
    isFamilyBoard,
  } from '../stores/noticeBoardStore'
  import {
    familyInfo,
    familyPermissions,
    FAMILY_PERMISSION,
  } from '../stores/familyStore'

  let { session }: { session: NoticeBoardSession } = $props()

  /** Longest edge of an attached picture, as the upload endpoint accepts. */
  const PICTURE_EDGE = 512

  let dialog = $state<HTMLDivElement>()
  let cork = $state<HTMLDivElement>()
  let fileInput = $state<HTMLInputElement>()
  let title = $state('')
  let body = $state('')
  let comment = $state('')
  let picture = $state<{ blob: Blob; url: string; hash: string | null } | null>(
    null
  )
  let pictureError = $state<MessageKey | null>(null)
  let uploading = $state(false)
  let spot = $state<[number, number] | null>(null)
  let dragging = $state(false)
  let confirmingRemove = $state(false)

  const view = $derived(session.view)
  /** Family boards are free to post on and follow the reader's rank. */
  const family = $derived(isFamilyBoard(session.board))
  const familyBits = $derived($familyInfo ? familyPermissions($familyInfo) : 0)
  const canWrite = $derived(
    !family || (familyBits & FAMILY_PERMISSION.BOARD_WRITE) !== 0
  )
  const moderator = $derived(
    family && (familyBits & FAMILY_PERMISSION.BOARD_MODERATE) !== 0
  )
  const hasPaper = $derived(family || $noticePaperCount > 0)
  /** Other writers' current papers, which a new one may barely cover. */
  const protectedSpots = $derived(
    session.posts
      .filter((p) => !p.mine && !p.expired)
      .map((p): [number, number] => [p.x, p.y])
  )
  const freeSpot = $derived(
    freePaperSpot(
      protectedSpots,
      session.posts.map((p): [number, number] => [p.x, p.y])
    )
  )
  const ownCurrent = $derived(
    session.posts.filter((p) => p.mine && !p.expired).length
  )
  const ghost = $derived<[number, number]>(
    spot ? clampPaperPosition(...spot) : (freeSpot ?? [500, 500])
  )
  const ghostAllowed = $derived(spotAllowed(ghost, protectedSpots))
  const canPost = $derived(
    !session.pending &&
      !uploading &&
      ghostAllowed &&
      hasPaper &&
      title.trim() !== '' &&
      (body.trim() !== '' || picture !== null)
  )
  const viewedIsMine = $derived(
    view.kind === 'post' &&
      session.posts.some((p) => p.id === view.post.id && p.mine)
  )
  const pinned = $derived([...session.posts].sort((a, b) => a.id - b.id))

  $effect(() => mountOverlay('noticeBoard', closeNoticeBoard))
  $effect(() => {
    tick().then(() => dialog?.focus())
  })
  $effect(() => {
    const url = picture?.url
    return () => {
      if (url) URL.revokeObjectURL(url)
    }
  })

  function paperStyle(x: number, y: number, id: number) {
    return [
      `left:${x / 10}%`,
      `top:${y / 10}%`,
      `width:${(PAPER_W / BOARD_FACE_W) * 100}%`,
      `height:${(PAPER_H / BOARD_FACE_H) * 100}%`,
      `transform:translate(-50%,-50%) rotate(${paperTilt(id)}rad)`,
    ].join(';')
  }

  const formatTime = (at: GameDateTime) => formatGameDateTime($t, at)

  function setView(next: NoticeBoardSession['view']) {
    noticeBoard.update((s) => (s ? { ...s, view: next, refusal: null } : s))
  }

  /** Refuses up front what would only be refused after writing. */
  function startWriting() {
    const refuse = (refusal: NoticeBoardRefusal) =>
      noticeBoard.update((s) => (s ? { ...s, refusal } : s))
    if (!canWrite) return refuse('no_family_rank')
    if (!hasPaper) return refuse('no_paper')
    if (ownCurrent >= NOTICE_POSTS_PER_WRITER) return refuse('too_many_posts')
    if (freeSpot === null) return refuse('board_full')
    setView({ kind: 'write' })
  }

  function markPending() {
    noticeBoard.update((s) => (s ? { ...s, pending: true, refusal: null } : s))
  }

  function openPost(postId: number) {
    markPending()
    networkManager.sendReadNoticePost(session.board, postId)
  }

  async function toPicturePng(file: File): Promise<Blob> {
    const bitmap = await createImageBitmap(file)
    const scale = Math.min(
      1,
      PICTURE_EDGE / Math.max(bitmap.width, bitmap.height)
    )
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.round(bitmap.width * scale))
    canvas.height = Math.max(1, Math.round(bitmap.height * scale))
    const ctx = canvas.getContext('2d')
    if (!ctx) throw new Error('no canvas')
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    bitmap.close()
    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob(resolve, 'image/png')
    )
    if (!blob) throw new Error('could not encode the picture')
    return blob
  }

  async function pickPicture(event: Event) {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    pictureError = null
    try {
      const blob = await toPicturePng(file)
      picture = { blob, url: URL.createObjectURL(blob), hash: null }
    } catch {
      pictureError = 'noticeBoard.pictureUnreadable'
    }
  }

  /** The stored hash, uploading the picture first if needed. */
  async function pictureHash(): Promise<string | null | undefined> {
    if (!picture) return null
    if (picture.hash) return picture.hash
    uploading = true
    const result = await uploadCapeTexture(picture.blob)
    uploading = false
    if (!('hash' in result)) {
      pictureError = 'noticeBoard.uploadFailed'
      return undefined
    }
    picture.hash = result.hash
    return result.hash
  }

  async function submitPost(event: SubmitEvent) {
    event.preventDefault()
    if (!canPost) return
    pictureError = null
    const image = await pictureHash()
    if (image === undefined) return
    markPending()
    networkManager.sendWriteNoticePost(session.board, title, body, image, ghost)
  }

  function submitComment(event: Event, postId: number) {
    event.preventDefault()
    if (!comment.trim() || session.pending) return
    markPending()
    networkManager.sendWriteNoticeComment(session.board, postId, comment)
  }

  function removePost(postId: number) {
    if (session.pending) return
    markPending()
    networkManager.sendRemoveNoticePost(session.board, postId)
  }

  function placeAt(event: PointerEvent) {
    if (!cork) return
    const rect = cork.getBoundingClientRect()
    spot = [
      ((event.clientX - rect.left) / rect.width) * PAPER_SCALE,
      ((event.clientY - rect.top) / rect.height) * PAPER_SCALE,
    ]
  }

  function startDrag(event: PointerEvent) {
    if (view.kind !== 'write') return
    event.preventDefault()
    dragging = true
    cork?.setPointerCapture(event.pointerId)
    placeAt(event)
  }

  function moveDrag(event: PointerEvent) {
    if (dragging) placeAt(event)
  }

  function endDrag(event: PointerEvent) {
    if (!dragging) return
    dragging = false
    cork?.releasePointerCapture(event.pointerId)
  }

  // A reply clears the form that produced it.
  let lastView: NoticeBoardSession['view'] | null = null
  $effect(() => {
    if (lastView && view !== lastView && view.kind === 'post') {
      if (lastView.kind === 'write') {
        title = ''
        body = ''
        picture = null
        spot = null
      } else {
        comment = ''
      }
    }
    if (view !== lastView) confirmingRemove = false
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
      <h2 id="board-title">
        {family ? $t('noticeBoard.familyTitle') : $t('noticeBoard.title')}
      </h2>
      <div class="header-actions">
        {#if !family}
          <span class="paper-count" title={$t('noticeBoard.paperHint')}>
            {$t('noticeBoard.papers', { count: $noticePaperCount })}
          </span>
        {/if}
        {#if view.kind === 'list'}
          <button class="primary" onclick={startWriting}
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

    {#if view.kind === 'write'}
      <form id="notice-write" class="write" onsubmit={submitPost}>
        <input
          bind:value={title}
          maxlength={NOTICE_TITLE_MAX_CHARS}
          placeholder={$t('noticeBoard.titlePlaceholder')}
        />
        <textarea
          bind:value={body}
          maxlength={NOTICE_BODY_MAX_CHARS}
          rows="4"
          placeholder={$t('noticeBoard.bodyPlaceholder')}></textarea>
        <div class="picture-row">
          <input
            bind:this={fileInput}
            type="file"
            accept="image/*"
            class="hidden-input"
            onchange={pickPicture}
          />
          <button
            type="button"
            class="secondary"
            onclick={() => fileInput?.click()}
            >{picture
              ? $t('noticeBoard.changePicture')
              : $t('noticeBoard.addPicture')}</button
          >
          {#if picture}
            <button
              type="button"
              class="secondary"
              onclick={() => (picture = null)}
              >{$t('noticeBoard.removePicture')}</button
            >
          {/if}
          <span class="muted">{body.length}/{NOTICE_BODY_MAX_CHARS}</span>
        </div>
        {#if pictureError}
          <p class="refusal">{$t(pictureError)}</p>
        {/if}
        {#if ghostAllowed}
          <p class="muted">{$t('noticeBoard.dragHint')}</p>
        {:else}
          <p class="refusal">{$t('noticeBoard.refused.covers_others')}</p>
        {/if}
      </form>
    {/if}

    {#if view.kind === 'post'}
      <article>
        {#if view.post.image}
          <img
            class="post-picture"
            src={capeTextureUrl(view.post.image)}
            alt=""
            onerror={(e) =>
              ((e.currentTarget as HTMLImageElement).hidden = true)}
          />
        {/if}
        <h3>{view.post.title}</h3>
        <div class="meta">
          {view.post.author} · {formatTime(view.post.written_at)}
          {#if view.post.expired}
            · <span class="expired-tag">{$t('noticeBoard.expired')}</span>
          {/if}
        </div>
        {#if view.post.body}
          <p class="body">{view.post.body}</p>
        {/if}
        {#if viewedIsMine || moderator}
          <div class="remove-row">
            {#if confirmingRemove}
              <span class="muted">{$t('noticeBoard.removeConfirm')}</span>
              <button
                class="danger"
                disabled={session.pending}
                onclick={() => removePost(view.post.id)}
                >{$t('noticeBoard.remove')}</button
              >
              <button
                class="secondary"
                onclick={() => (confirmingRemove = false)}
                >{$t('noticeBoard.removeCancel')}</button
              >
            {:else}
              <button
                class="secondary"
                onclick={() => (confirmingRemove = true)}
                >{$t('noticeBoard.remove')}</button
              >
            {/if}
          </div>
        {/if}
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
    {:else}
      <div class="frame">
        <div
          class="cork"
          class:placing={view.kind === 'write'}
          role="presentation"
          bind:this={cork}
          onpointerdown={startDrag}
          onpointermove={moveDrag}
          onpointerup={endDrag}
          onpointercancel={endDrag}
        >
          {#each pinned as post (post.id)}
            <button
              class="paper"
              class:old={post.expired}
              style={paperStyle(post.x, post.y, post.id)}
              disabled={view.kind === 'write'}
              onclick={() => openPost(post.id)}
            >
              <span class="pin"></span>
              {#if post.image}
                <img
                  src={capeTextureUrl(post.image)}
                  alt=""
                  draggable="false"
                  onerror={(e) =>
                    ((e.currentTarget as HTMLImageElement).hidden = true)}
                />
              {/if}
              <span class="paper-title">{post.title}</span>
              {#if post.excerpt}
                <span class="paper-excerpt">{post.excerpt}</span>
              {/if}
              <span class="paper-meta"
                >{post.author}{#if post.comment_count > 0}
                  · {post.comment_count}{/if}</span
              >
              {#if post.expired}
                <span class="stamp">{$t('noticeBoard.expired')}</span>
              {/if}
            </button>
          {/each}
          {#if view.kind === 'write'}
            <div
              class="paper ghost"
              class:blocked={!ghostAllowed}
              style={paperStyle(ghost[0], ghost[1], 0)}
            >
              <span class="pin"></span>
              {#if picture}
                <img src={picture.url} alt="" draggable="false" />
              {/if}
              <span class="paper-title"
                >{title || $t('noticeBoard.titlePlaceholder')}</span
              >
              {#if body}
                <span class="paper-excerpt">{noticeExcerpt(body)}</span>
              {/if}
            </div>
          {:else if session.posts.length === 0}
            <p class="empty">{$t('noticeBoard.empty')}</p>
          {/if}
        </div>
      </div>
      {#if view.kind === 'write'}
        <div class="row-end">
          <span class="muted">
            {#if !hasPaper}
              {$t('noticeBoard.paperHint')}
            {/if}
          </span>
          <button
            class="primary"
            type="submit"
            form="notice-write"
            disabled={!canPost}>{$t('noticeBoard.submit')}</button
          >
        </div>
      {/if}
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
    width: min(700px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
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
  .board-dialog,
  textarea {
    scrollbar-width: thin;
    scrollbar-color: rgba(171, 147, 103, 0.5) transparent;
  }
  @supports selector(::-webkit-scrollbar) {
    .board-dialog,
    textarea {
      scrollbar-width: auto;
      scrollbar-color: auto;
    }
    .board-dialog::-webkit-scrollbar,
    textarea::-webkit-scrollbar {
      width: 12px;
    }
    .board-dialog::-webkit-scrollbar-track,
    textarea::-webkit-scrollbar-track {
      margin: 8px 0;
      background: transparent;
    }
    textarea::-webkit-scrollbar-track {
      margin: 4px 0;
    }
    .board-dialog::-webkit-scrollbar-thumb,
    textarea::-webkit-scrollbar-thumb {
      border: 3px solid transparent;
      border-radius: 999px;
      background: rgba(171, 147, 103, 0.5);
      background-clip: padding-box;
    }
    .board-dialog::-webkit-scrollbar-thumb:hover,
    textarea::-webkit-scrollbar-thumb:hover {
      background: rgba(205, 185, 143, 0.75);
      background-clip: padding-box;
    }
    .board-dialog::-webkit-scrollbar-button,
    textarea::-webkit-scrollbar-button {
      display: none;
    }
    textarea::-webkit-scrollbar-corner {
      background: transparent;
    }
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .paper-count {
    padding: 0 6px;
    color: #cdb98f;
    font-size: 12px;
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
  .frame {
    padding: 10px;
    border-radius: 4px;
    background: #4a3322;
    box-shadow: inset 0 0 0 2px #2e1f14;
  }
  .cork {
    position: relative;
    aspect-ratio: 1.66 / 1.4;
    overflow: hidden;
    border-radius: 2px;
    background-color: #8a6a45;
    background-image:
      radial-gradient(rgba(60, 40, 22, 0.35) 1px, transparent 1.5px),
      radial-gradient(rgba(255, 230, 190, 0.12) 1px, transparent 1.5px);
    background-position:
      0 0,
      5px 7px;
    background-size:
      11px 13px,
      9px 11px;
    touch-action: none;
  }
  .cork.placing {
    cursor: crosshair;
  }
  .cork.placing .paper {
    pointer-events: none;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    color: #f1e3c6;
    font-size: 13px;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
  }
  .paper {
    position: absolute;
    display: flex;
    flex-direction: column;
    gap: 3px;
    box-sizing: border-box;
    padding: 12px 8px 6px;
    overflow: hidden;
    border: none;
    border-radius: 1px;
    background: #efe5cc;
    box-shadow: 1px 2px 4px rgba(0, 0, 0, 0.45);
    color: #2b2116;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .paper:disabled {
    opacity: 0.55;
    cursor: inherit;
  }
  .paper:not(:disabled):hover {
    filter: brightness(1.05);
    z-index: 1;
  }
  .paper.old {
    background: #d4bd86;
    color: #4d3d26;
  }
  .paper.ghost {
    z-index: 2;
    outline: 2px dashed #c58a3a;
    outline-offset: 2px;
  }
  .paper.ghost.blocked {
    outline-color: #c4453a;
    opacity: 0.75;
  }
  .pin {
    position: absolute;
    top: 4px;
    left: 50%;
    width: 8px;
    height: 8px;
    margin-left: -4px;
    border-radius: 50%;
    background: #8a2a1c;
    box-shadow: 0 1px 1px rgba(0, 0, 0, 0.5);
  }
  .paper img {
    width: 100%;
    max-height: 55%;
    object-fit: contain;
  }
  .paper-title {
    display: -webkit-box;
    overflow: hidden;
    font-size: 12px;
    font-weight: 700;
    line-height: 1.25;
    word-break: break-word;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
  .paper-excerpt {
    flex: 1;
    overflow: hidden;
    font-size: 10px;
    line-height: 1.3;
    word-break: break-word;
  }
  .paper-meta {
    color: #6d5a42;
    font-size: 9px;
  }
  .stamp {
    position: absolute;
    right: 4px;
    bottom: 14px;
    padding: 1px 4px;
    border: 1.5px solid #9b3a2a;
    border-radius: 3px;
    color: #9b3a2a;
    font-size: 9px;
    font-weight: 700;
    transform: rotate(-12deg);
  }
  .expired-tag {
    color: #d08a5a;
  }
  .post-picture {
    display: block;
    max-width: 100%;
    max-height: 360px;
    margin: 0 auto 10px;
    border-radius: 4px;
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
  .meta {
    color: #998a73;
    font-size: 11px;
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
  .picture-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .picture-row .muted {
    margin-left: auto;
  }
  .hidden-input {
    display: none;
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
    gap: 8px;
  }
  .remove-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 10px;
  }
  button.danger {
    border: 1px solid #b95749;
    background: #6e2a20;
    color: #ffe9e2;
  }
  button.primary,
  button.secondary,
  button.danger {
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
