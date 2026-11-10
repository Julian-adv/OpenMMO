<script lang="ts">
  import { tick } from 'svelte'
  import { t, type MessageKey } from '../i18n'
  import { formatGameDateTime } from '../data/gameCalendar'
  import { networkManager } from '../network/socket'
  import { mountOverlay } from '../stores/overlayStack'
  import type { GameDateTime } from '../stores/noticeBoardStore'
  import {
    closeMailbox,
    letterPaperCount,
    mailSession,
    LETTER_BODY_MAX_CHARS,
    type MailSession,
    type MailView,
  } from '../stores/mailStore'

  let { session }: { session: MailSession } = $props()

  let dialog = $state<HTMLDivElement>()
  let recipient = $state('')
  let body = $state('')

  const view = $derived(session.view)
  const unread = $derived(session.letters.filter((l) => !l.read).length)

  $effect(() => mountOverlay('mailbox', closeMailbox))
  $effect(() => {
    tick().then(() => dialog?.focus())
  })

  const formatTime = (at: GameDateTime) => formatGameDateTime($t, at)

  function setView(next: MailView) {
    mailSession.update((s) => (s ? { ...s, view: next, notice: null } : s))
  }

  function startWriting(to: string) {
    if ($letterPaperCount === 0) {
      mailSession.update((s) =>
        s ? { ...s, notice: { refused: 'no_letter_paper' } } : s
      )
      return
    }
    recipient = to
    body = ''
    setView({ kind: 'write' })
  }

  function markPending() {
    mailSession.update((s) => (s ? { ...s, pending: true, notice: null } : s))
  }

  function openLetter(letterId: number) {
    markPending()
    networkManager.sendReadMail(session.mailbox, letterId)
  }

  function deleteLetter(letterId: number) {
    markPending()
    networkManager.sendDeleteMail(session.mailbox, letterId)
  }

  function submit(event: SubmitEvent) {
    event.preventDefault()
    if (!recipient.trim() || !body.trim() || session.pending) return
    markPending()
    networkManager.sendMail(session.mailbox, recipient, body)
  }
</script>

<div class="mail-backdrop">
  <div
    class="mail-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="mail-title"
    tabindex="-1"
    bind:this={dialog}
  >
    <header>
      <h2 id="mail-title">{$t('mail.title')}</h2>
      <div class="header-actions">
        {#if view.kind === 'list'}
          <button class="primary" onclick={() => startWriting('')}
            >{$t('mail.write')}</button
          >
        {:else}
          <button class="secondary" onclick={() => setView({ kind: 'list' })}
            >{$t('mail.back')}</button
          >
        {/if}
        <button
          class="secondary"
          aria-label={$t('mail.close')}
          onclick={closeMailbox}>×</button
        >
      </div>
    </header>

    {#if session.notice && 'refused' in session.notice}
      <p class="refusal">
        {$t(`mail.refused.${session.notice.refused}` as MessageKey)}
      </p>
    {:else if session.notice}
      <p class="sent">{$t('mail.sent', { name: session.notice.sentTo })}</p>
    {/if}

    {#if view.kind === 'list'}
      {#if session.letters.length === 0}
        <p class="muted">{$t('mail.empty')}</p>
      {:else}
        <p class="muted">
          {$t('mail.count', { count: session.letters.length, unread })}
        </p>
        <ul class="letters">
          {#each session.letters as letter (letter.id)}
            <li>
              <button
                class="letter-row"
                class:unread={!letter.read}
                onclick={() => openLetter(letter.id)}
              >
                <span class="sender">{letter.sender}</span>
                <span class="meta">{formatTime(letter.written_at)}</span>
                <span class="excerpt">{letter.excerpt}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if view.kind === 'write'}
      <form class="write" onsubmit={submit}>
        <input
          bind:value={recipient}
          maxlength="32"
          placeholder={$t('mail.recipientPlaceholder')}
        />
        <textarea
          bind:value={body}
          maxlength={LETTER_BODY_MAX_CHARS}
          rows="8"
          placeholder={$t('mail.bodyPlaceholder')}></textarea>
        <div class="row-end">
          <span class="muted"
            >{$t('mail.papers', { count: $letterPaperCount })} · {body.length}/{LETTER_BODY_MAX_CHARS}</span
          >
          <button
            class="primary"
            type="submit"
            disabled={session.pending ||
              !recipient.trim() ||
              !body.trim() ||
              $letterPaperCount === 0}>{$t('mail.send')}</button
          >
        </div>
        {#if $letterPaperCount === 0}
          <p class="muted">{$t('mail.paperHint')}</p>
        {/if}
      </form>
    {:else}
      <article>
        <div class="meta">
          {$t('mail.from', { name: view.letter.sender })} · {formatTime(
            view.letter.written_at
          )}
        </div>
        <p class="body">{view.letter.body}</p>
      </article>
      <div class="row-end">
        <button
          class="secondary"
          disabled={session.pending}
          onclick={() => deleteLetter(view.letter.id)}
          >{$t('mail.delete')}</button
        >
        <button class="primary" onclick={() => startWriting(view.letter.sender)}
          >{$t('mail.reply')}</button
        >
      </div>
    {/if}
  </div>
</div>

<style>
  .mail-backdrop {
    position: absolute;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    padding: 16px;
    box-sizing: border-box;
    background: rgba(8, 5, 4, 0.45);
  }
  .mail-dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: min(480px, calc(100vw - 32px));
    max-height: min(600px, calc(100vh - 32px));
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
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .letters li + li {
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .letter-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px 8px;
    width: 100%;
    padding: 8px 4px;
    border: none;
    background: none;
    color: #b9ab94;
    text-align: left;
    cursor: pointer;
  }
  .letter-row.unread {
    color: inherit;
  }
  .letter-row.unread .sender::before {
    content: '● ';
    color: #e2b05a;
  }
  .letter-row:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .sender {
    font-size: 14px;
    font-weight: 700;
  }
  .excerpt {
    grid-column: 1 / -1;
    overflow: hidden;
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: #998a73;
    font-size: 11px;
  }
  .muted {
    margin: 0;
    color: #998a73;
    font-size: 12px;
  }
  .refusal,
  .sent {
    margin: 0;
    padding: 8px 10px;
    font-size: 12px;
  }
  .refusal {
    border-left: 3px solid #b95749;
    background: rgba(145, 55, 44, 0.16);
    color: #e3c7bd;
  }
  .sent {
    border-left: 3px solid #6f9a52;
    background: rgba(80, 120, 60, 0.16);
    color: #d3e3c4;
  }
  .body {
    margin: 10px 0 0;
    font-size: 13px;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }
  form.write {
    display: flex;
    flex-direction: column;
    gap: 8px;
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
