<script lang="ts">
  import { t } from '../i18n'
  import {
    familyInfo,
    familyStorageLogVisible,
    hasFamilyEmblem,
    familyPanelVisible,
    familyPermissions,
    familyDefaultLabelKey,
    validFamilyRankLabel,
    FAMILY_DEFAULT_LABELS,
    type FamilyPermissionKey,
    FAMILY_MAX_MEMBERS,
    FAMILY_PERMISSION,
    FAMILY_FOUND_STAMPS,
    FAMILY_REJOIN_COOLDOWN_HOURS,
    FAMILY_RANK_HEAD,
    FAMILY_RANK_GUEST,
    FAMILY_RANK_LABEL_MAX_CHARS,
    type FamilyEstate,
    type FamilyMember,
  } from '../stores/familyStore'
  import { gameStore } from '../stores/gameStore'
  import { playerGold } from '../stores/inventoryStore'
  import { classIconPath } from '../stores/partyStore'
  import { networkManager } from '../network/socket'
  import { draggablePanel } from '../actions/draggablePanel'
  import { hintTooltip } from '../actions/hintTooltip'
  import { parseGold } from '../utils/currency'
  import { capeTextureUrl } from '../utils/networkUtils'
  import CapeTextureDialog from './CapeTextureDialog.svelte'

  const family = $derived($familyInfo)
  const myRank = $derived(family?.my_rank ?? FAMILY_RANK_GUEST)
  const perms = $derived(family ? familyPermissions(family) : 0)
  const isHead = $derived(family !== null && myRank === FAMILY_RANK_HEAD)
  const canInvite = $derived((perms & FAMILY_PERMISSION.INVITE) !== 0)
  const canKick = $derived((perms & FAMILY_PERMISSION.KICK) !== 0)
  const canEditRanks = $derived((perms & FAMILY_PERMISSION.EDIT_RANKS) !== 0)
  const canDeposit = $derived(
    (perms & FAMILY_PERMISSION.TREASURY_DEPOSIT) !== 0
  )
  const canEditEmblem = $derived((perms & FAMILY_PERMISSION.EDIT_EMBLEM) !== 0)
  const hasEmblem = $derived(hasFamilyEmblem(family))
  let choosingPicture = $state(false)
  function setEmblem(texture: string | null, color: string | null) {
    networkManager.sendSetFamilyEmblem(texture, color)
  }

  const selfName = $derived($gameStore.currentPlayer?.name)
  const full = $derived((family?.members.length ?? 0) >= FAMILY_MAX_MEMBERS)

  const rankLabel = (label: string) => {
    const key = familyDefaultLabelKey(label)
    return key ? $t(key) : label
  }
  const labelOfRank = (rank: number) =>
    rankLabel(family?.ranks[rank]?.label ?? '')
  /** Ranks I may move a member to: 1..3 and strictly below mine. */
  const assignableRanks = $derived(
    [1, 2, 3].filter((rank) => rank > myRank && family?.ranks[rank])
  )
  const outranks = (member: FamilyMember) =>
    member.name !== selfName && myRank < member.rank

  let confirm = $state<{
    text: string
    label: string
    run: () => void
  } | null>(null)
  // A change of members or ranks may invalidate what the confirm describes;
  // a member merely coming online does not.
  const rosterKey = $derived(
    family
      ? family.members.map((m) => `${m.character_id}:${m.rank}`).join(',')
      : ''
  )
  $effect(() => {
    void rosterKey
    confirm = null
  })

  function runConfirm() {
    confirm?.run()
    confirm = null
  }

  let inviteName = $state('')
  function submitInvite() {
    const name = inviteName.trim()
    if (name) networkManager.sendInviteToFamily(name)
    inviteName = ''
  }

  function inputKeydown(event: KeyboardEvent, submit: () => void) {
    event.stopPropagation()
    if (event.key === 'Enter') submit()
  }

  function askKick(member: FamilyMember) {
    confirm = {
      text: $t('family.kickConfirm', {
        name: member.name,
        hours: FAMILY_REJOIN_COOLDOWN_HOURS,
      }),
      label: $t('family.kick'),
      run: () => networkManager.sendKickFamilyMember(member.character_id),
    }
  }

  function askTransfer(member: FamilyMember) {
    confirm = {
      text: $t('family.transferConfirm', { name: member.name }),
      label: $t('family.transfer'),
      run: () => networkManager.sendTransferFamilyHead(member.character_id),
    }
  }

  function askLeave() {
    if (!family) return
    confirm = {
      text: $t('family.leaveConfirm', {
        family: family.name,
        hours: FAMILY_REJOIN_COOLDOWN_HOURS,
      }),
      label: $t('family.leave'),
      run: () => networkManager.sendLeaveFamily(),
    }
  }

  function askDetach() {
    confirm = {
      text: $t('family.detachConfirm'),
      label: $t('family.detachEstate'),
      run: () => networkManager.sendAttachEstateToFamily(false),
    }
  }

  let ranksOpen = $state(false)
  let drafts = $state<{ label: string; permissions: number }[]>([])
  const ranksKey = $derived(JSON.stringify(family?.ranks ?? []))
  $effect(() => {
    const ranks = JSON.parse(ranksKey) as {
      label: string
      permissions: number
    }[]
    drafts = ranks.map((rank) => ({
      label: familyDefaultLabelKey(rank.label) ? '' : rank.label,
      permissions: rank.permissions,
    }))
  })
  const editableRank = (rank: number) =>
    canEditRanks && (rank > myRank || (isHead && rank === FAMILY_RANK_HEAD))
  const draftLabelValid = (rank: number) => {
    const label = drafts[rank]?.label.trim() ?? ''
    return label === '' || validFamilyRankLabel(label)
  }
  const draftChanged = (rank: number) => {
    const draft = drafts[rank]
    const stored = family?.ranks[rank]
    if (!draft || !stored) return false
    const label = draft.label.trim() || FAMILY_DEFAULT_LABELS[rank]
    return label !== stored.label || draft.permissions !== stored.permissions
  }

  function togglePermission(rank: number, bit: number) {
    drafts[rank].permissions ^= bit
  }

  function saveRank(rank: number) {
    const draft = drafts[rank]
    if (!draft || !draftLabelValid(rank)) return
    networkManager.sendSetFamilyRank(
      rank,
      draft.label.trim() || FAMILY_DEFAULT_LABELS[rank],
      draft.permissions
    )
  }

  let depositText = $state<Record<number, string>>({})
  const depositAmount = (estate: FamilyEstate) => {
    const amount = parseGold(depositText[estate.estate_id] ?? '')
    return amount !== null &&
      Number.isSafeInteger(amount) &&
      amount > 0 &&
      amount <= $playerGold
      ? amount
      : null
  }

  function submitDeposit(estate: FamilyEstate) {
    const amount = depositAmount(estate)
    if (amount === null) return
    networkManager.sendDepositFamilyEstate(estate.estate_id, amount)
    depositText[estate.estate_id] = ''
  }
</script>

{#if $familyPanelVisible}
  <div
    class="family-panel"
    aria-label={$t('family.title')}
    use:draggablePanel={'family'}
  >
    <div class="panel-header" data-drag-handle>
      <span class="panel-title">{family?.name ?? $t('family.title')}</span>
      {#if family}
        <span class="count">{family.members.length}/{FAMILY_MAX_MEMBERS}</span>
      {/if}
      <button
        class="close-btn"
        use:hintTooltip={$t('common.close')}
        aria-label={$t('common.close')}
        onclick={() => familyPanelVisible.set(false)}>×</button
      >
    </div>

    {#if !family}
      <p class="note">
        {$t('family.noneFound', { count: FAMILY_FOUND_STAMPS })}
      </p>
      <p class="note">{$t('family.noneJoin')}</p>
    {:else}
      <div class="body">
        <div class="line">
          <span class="dim">{$t('family.myRank')}</span>
          <span>{labelOfRank(myRank)}</span>
        </div>
        {#if family.can_claim_head}
          <button
            class="text-btn wide"
            onclick={() => networkManager.sendClaimFamilyHead()}
            use:hintTooltip={$t('family.claimHeadHint')}
            >{$t('family.claimHead')}</button
          >
        {/if}

        <div class="section-title">{$t('family.members')}</div>
        {#each family.members as member (member.character_id)}
          <div class="member-row" class:offline={!member.online}>
            <span
              class="status-dot"
              class:on={member.online}
              use:hintTooltip={member.online
                ? $t('friends.online')
                : $t('friends.offline')}
            ></span>
            {#if classIconPath(member.class)}
              <img
                class="class-icon"
                src={classIconPath(member.class)}
                alt={$t(`class.${member.class}`)}
                use:hintTooltip={$t(`class.${member.class}`)}
                width="14"
                height="14"
              />
            {/if}
            <span class="member-name" use:hintTooltip={member.name}
              >{member.name}</span
            >
            <span class="dim small">{$t('stat.level')} {member.level}</span>
            {#if canEditRanks && outranks(member) && assignableRanks.length > 0}
              <select
                class="rank-select"
                aria-label={$t('family.changeRank')}
                use:hintTooltip={$t('family.changeRank')}
                value={member.rank}
                onchange={(event) =>
                  networkManager.sendSetFamilyMemberRank(
                    member.character_id,
                    Number(event.currentTarget.value)
                  )}
              >
                {#each assignableRanks as rank (rank)}
                  <option value={rank}>{labelOfRank(rank)}</option>
                {/each}
              </select>
            {:else}
              <span class="rank-tag">{labelOfRank(member.rank)}</span>
            {/if}
            {#if isHead && member.name !== selfName}
              <button
                class="row-btn"
                use:hintTooltip={$t('family.transfer')}
                aria-label={$t('family.transfer')}
                onclick={() => askTransfer(member)}>♛</button
              >
            {/if}
            {#if canKick && outranks(member)}
              <button
                class="row-btn danger"
                use:hintTooltip={$t('family.kick')}
                aria-label={$t('family.kick')}
                onclick={() => askKick(member)}>×</button
              >
            {/if}
          </div>
        {/each}

        {#if canInvite}
          <div class="input-row">
            <input
              class="text-input"
              placeholder={$t('friends.playerName')}
              aria-label={$t('family.invite')}
              maxlength="32"
              disabled={full}
              bind:value={inviteName}
              onkeydown={(event) => inputKeydown(event, submitInvite)}
            />
            <button
              class="text-btn"
              disabled={full || !inviteName.trim()}
              onclick={submitInvite}>{$t('family.invite')}</button
            >
          </div>
        {/if}

        {#if canEditRanks}
          <button
            class="section-title toggle"
            aria-expanded={ranksOpen}
            onclick={() => (ranksOpen = !ranksOpen)}
            >{ranksOpen ? '▾' : '▸'} {$t('family.ranks')}</button
          >
          {#if ranksOpen}
            {#each family.ranks as rank, index (index)}
              {#if editableRank(index) && drafts[index]}
                <div class="rank-edit">
                  <div class="input-row">
                    <input
                      class="text-input"
                      class:invalid={!draftLabelValid(index)}
                      aria-label={$t('family.rankLabel')}
                      use:hintTooltip={$t('family.rankLabelHint')}
                      placeholder={$t(
                        familyDefaultLabelKey(FAMILY_DEFAULT_LABELS[index]) ??
                          'family.rankLabel'
                      )}
                      maxlength={FAMILY_RANK_LABEL_MAX_CHARS}
                      bind:value={drafts[index].label}
                      onkeydown={(event) =>
                        inputKeydown(event, () => saveRank(index))}
                    />
                    <button
                      class="text-btn"
                      disabled={!draftChanged(index) || !draftLabelValid(index)}
                      onclick={() => saveRank(index)}
                      >{$t('family.save')}</button
                    >
                  </div>
                  {#if !draftLabelValid(index)}
                    <div class="warn small">{$t('family.rankLabelRule')}</div>
                  {/if}
                  {#if index !== FAMILY_RANK_HEAD}
                    <div class="perm-grid">
                      {#each Object.keys(FAMILY_PERMISSION) as FamilyPermissionKey[] as key (key)}
                        {@const bit = FAMILY_PERMISSION[key]}
                        <label class="perm" class:locked={(perms & bit) === 0}>
                          <input
                            type="checkbox"
                            checked={(drafts[index].permissions & bit) !== 0}
                            disabled={(perms & bit) === 0}
                            onchange={() => togglePermission(index, bit)}
                          />
                          {$t(`family.perm.${key}`)}
                        </label>
                      {/each}
                    </div>
                  {:else}
                    <div class="dim small">{rankLabel(rank.label)}</div>
                  {/if}
                </div>
              {/if}
            {/each}
          {/if}
        {/if}

        <div class="section-title">{$t('family.estates')}</div>
        {#if family.estates.length === 0}
          <div class="dim">{$t('family.noEstates')}</div>
        {/if}
        {#each family.estates as estate (estate.estate_id)}
          <div class="estate">
            <div class="line">
              <span class="member-name"
                >{$t('family.estateOf', { name: estate.owner_name })}</span
              >
              <span class="dim small"
                >{$t('family.plots', { n: estate.plots })}</span
              >
              {#if estate.estate_id === family.seat_estate_id}
                <span
                  class="seat-tag small"
                  use:hintTooltip={$t('family.seatHint')}
                  >{$t('family.seat')}</span
                >
              {:else if isHead}
                <button
                  class="text-btn"
                  use:hintTooltip={$t('family.seatHint')}
                  onclick={() =>
                    networkManager.sendSetFamilySeat(estate.estate_id)}
                  >{$t('family.makeSeat')}</button
                >
              {/if}
              {#if estate.missed > 0}
                <span class="warn small"
                  >{$t('family.overdue', { n: estate.missed })}</span
                >
              {/if}
              {#if estate.mine}
                <button class="text-btn" onclick={askDetach}
                  >{$t('family.detachEstate')}</button
                >
              {/if}
            </div>
            {#if canDeposit}
              <div class="input-row">
                <input
                  class="text-input"
                  placeholder="1g 20s 30c"
                  aria-label={$t('family.deposit')}
                  use:hintTooltip={$t('family.depositHint')}
                  autocomplete="off"
                  bind:value={depositText[estate.estate_id]}
                  onkeydown={(event) =>
                    inputKeydown(event, () => submitDeposit(estate))}
                />
                <button
                  class="text-btn"
                  disabled={depositAmount(estate) === null}
                  use:hintTooltip={$t('family.depositHint')}
                  onclick={() => submitDeposit(estate)}
                  >{$t('family.deposit')}</button
                >
              </div>
            {/if}
          </div>
        {/each}
        {#if family.can_attach_estate}
          <button
            class="text-btn wide"
            onclick={() => networkManager.sendAttachEstateToFamily(true)}
            >{$t('family.attachEstate')}</button
          >
        {/if}

        <div class="section-title">{$t('family.emblem')}</div>
        <div class="line">
          {#if family.emblem_texture}
            <img
              class="emblem-thumb"
              src={capeTextureUrl(family.emblem_texture)}
              alt={$t('family.emblem')}
            />
          {/if}
          {#if family.emblem_color}
            <span
              class="emblem-swatch"
              style="background:{family.emblem_color}"
              use:hintTooltip={family.emblem_color}
            ></span>
          {/if}
          {#if !hasEmblem}
            <span class="dim small">{$t('family.noEmblem')}</span>
          {/if}
        </div>
        {#if hasEmblem}
          <label
            class="perm cape-toggle"
            use:hintTooltip={$t('family.capeEmblemHint')}
          >
            <input
              type="checkbox"
              checked={family.my_cape_emblem}
              onchange={(event) =>
                networkManager.sendSetFamilyCapeEmblem(
                  event.currentTarget.checked
                )}
            />
            {$t('family.capeEmblem')}
          </label>
        {/if}
        {#if canEditEmblem}
          <div class="input-row">
            <input
              type="color"
              class="emblem-color"
              aria-label={$t('family.emblemColor')}
              use:hintTooltip={$t('family.emblemColor')}
              value={family.emblem_color ?? '#6d1720'}
              onchange={(event) =>
                setEmblem(family.emblem_texture, event.currentTarget.value)}
            />
            <button class="text-btn" onclick={() => (choosingPicture = true)}
              >{$t('family.emblemPicture')}</button
            >
            {#if hasEmblem}
              <button class="text-btn" onclick={() => setEmblem(null, null)}
                >{$t('family.emblemClear')}</button
              >
            {/if}
          </div>
          <div class="dim small">{$t('family.emblemHint')}</div>
        {/if}

        {#if isHead}
          <div class="dim small">{$t('family.mergeAtChapel')}</div>
        {/if}

        <button
          class="text-btn wide"
          onclick={() => familyStorageLogVisible.update((open) => !open)}
          >{$t('family.storageLog')}</button
        >

        <div class="leave-row">
          <button class="text-btn danger" disabled={isHead} onclick={askLeave}
            >{$t('family.leave')}</button
          >
          {#if isHead && family.members.length > 1}
            <span class="dim small">{$t('family.headCannotLeave')}</span>
          {/if}
        </div>
        {#if isHead}
          <div class="dim small">{$t('family.registrarHint')}</div>
        {/if}
      </div>
    {/if}
  </div>

  {#if choosingPicture && family}
    <CapeTextureDialog
      title={$t('family.emblemPictureTitle')}
      intro={$t('family.emblemPictureIntro')}
      confirmLabel={$t('family.save')}
      onConfirm={(hash) => {
        setEmblem(hash, family.emblem_color)
        choosingPicture = false
      }}
      onCancel={() => (choosingPicture = false)}
    />
  {/if}

  {#if confirm}
    <div class="confirm-backdrop" role="presentation">
      <div class="confirm-box" role="alertdialog" aria-label={confirm.label}>
        <p class="confirm-text">{confirm.text}</p>
        <div class="confirm-actions">
          <button class="confirm-run" onclick={runConfirm}
            >{confirm.label}</button
          >
          <button class="confirm-cancel" onclick={() => (confirm = null)}>
            {$t('common.cancel')}
          </button>
        </div>
      </div>
    </div>
  {/if}
{/if}

<style>
  .family-panel {
    position: fixed;
    right: 276px;
    top: 45%;
    transform: translateY(-50%);
    z-index: 40;
    width: 300px;
    max-height: 76vh;
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
    color: #c8b88a;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count,
  .dim {
    color: #7f8f9f;
  }

  .small {
    font-size: 10px;
  }

  .warn {
    color: #efae87;
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

  .note {
    margin: 2px 2px 6px;
    color: #9fb2c3;
    line-height: 1.6;
  }

  .body {
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: rgba(113, 128, 150, 0.5) transparent;
  }

  .body::-webkit-scrollbar {
    width: 8px;
  }

  .body::-webkit-scrollbar-track {
    background: transparent;
  }

  .body::-webkit-scrollbar-thumb {
    background: rgba(113, 128, 150, 0.5);
    border-radius: 999px;
  }

  .body::-webkit-scrollbar-thumb:hover {
    background: rgba(160, 174, 192, 0.7);
  }

  .line,
  .member-row,
  .input-row,
  .leave-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .member-row {
    line-height: 1.9;
    white-space: nowrap;
  }

  .member-row.offline {
    color: #7f8f9f;
  }

  .input-row,
  .leave-row {
    margin-top: 6px;
  }

  .section-title {
    display: block;
    width: 100%;
    margin: 10px 0 4px;
    padding: 6px 0 0;
    border: none;
    border-top: 1px solid rgba(255, 255, 255, 0.12);
    background: none;
    color: #c8b88a;
    font-family: inherit;
    font-size: 11px;
    font-weight: 700;
    text-align: left;
  }

  .section-title.toggle {
    cursor: pointer;
  }

  .section-title.toggle:focus {
    outline: none;
  }

  /* Keyboard focus only, drawn inside so the scroll area does not clip it. */
  .section-title.toggle:focus-visible {
    outline: 1px solid rgba(200, 184, 138, 0.6);
    outline-offset: -2px;
    border-radius: 4px;
  }

  .emblem-thumb {
    width: 28px;
    height: 44px;
    object-fit: cover;
    border-radius: 2px;
    border: 1px solid rgba(255, 255, 255, 0.2);
  }

  .emblem-swatch {
    width: 18px;
    height: 18px;
    border-radius: 3px;
    border: 1px solid rgba(255, 255, 255, 0.3);
  }

  .emblem-color {
    width: 32px;
    height: 24px;
    padding: 0;
    border: none;
    background: none;
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

  .member-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rank-tag {
    color: #c8b88a;
    font-size: 10px;
  }

  .rank-select,
  .text-input {
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

  .rank-select {
    max-width: 96px;
    padding: 1px 2px;
    font-size: 10px;
  }

  .rank-select option {
    background: #10161c;
  }

  .text-input {
    flex: 1;
  }

  .text-input:focus {
    border-color: rgba(200, 184, 138, 0.6);
  }

  .text-input.invalid {
    border-color: rgba(239, 174, 135, 0.7);
  }

  .text-btn,
  .row-btn {
    flex: none;
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 3px;
    color: #c8b88a;
    font-family: inherit;
    font-size: 11px;
    line-height: 1;
    padding: 3px 6px;
    cursor: pointer;
  }

  .text-btn.wide {
    display: block;
    width: 100%;
    margin-top: 6px;
    padding: 5px 6px;
  }

  .row-btn {
    width: 18px;
    height: 18px;
    padding: 0;
    color: #9fb2c3;
    font-size: 10px;
  }

  .text-btn:hover:not(:disabled),
  .row-btn:hover:not(:disabled) {
    color: #fff;
    border-color: rgba(255, 255, 255, 0.4);
  }

  .text-btn:disabled,
  .row-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .danger:hover:not(:disabled) {
    color: #ff8f8f;
    border-color: rgba(255, 143, 143, 0.5);
  }

  .rank-edit,
  .estate {
    margin-bottom: 8px;
  }

  .perm-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 8px;
    margin-top: 4px;
  }

  .perm {
    display: flex;
    align-items: center;
    gap: 4px;
    color: #c5d0da;
    font-size: 11px;
  }

  .seat-tag {
    color: #c8b88a;
    font-weight: 700;
  }

  .cape-toggle {
    margin: 4px 0;
  }

  .perm.locked {
    opacity: 0.45;
  }

  .perm input {
    margin: 0;
  }

  .confirm-backdrop {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.35);
    pointer-events: auto;
  }

  .confirm-box {
    max-width: 320px;
    padding: 14px 16px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 8px;
    background: rgba(6, 10, 14, 0.95);
    color: #e6edf3;
    font-family: 'Noto Sans KR', sans-serif;
    font-size: 13px;
    line-height: 1.5;
  }

  .confirm-text {
    margin: 0 0 12px;
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .confirm-actions button {
    padding: 4px 12px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 4px;
    background: none;
    color: #e6edf3;
    font-family: inherit;
    font-size: 12px;
    cursor: pointer;
  }

  .confirm-actions .confirm-run {
    border-color: rgba(255, 143, 143, 0.6);
    color: #ff8f8f;
  }
</style>
