<script lang="ts">
  import type { AccountCharacter } from '../network/socket'
  import { titleName } from '../data/titleDefs'
  import { t } from '../i18n'
  import { character_max_mana } from '../wasm/onlinerpg_shared'

  interface Props {
    character: AccountCharacter
    compact?: boolean
  }

  let { character, compact = false }: Props = $props()
  const attributes = ['str', 'dex', 'con', 'int', 'wis', 'cha'] as const
  const maxMp = $derived(
    character_max_mana(
      character.class,
      character.attributes.wis,
      character.level
    )
  )
</script>

<span class="character-summary" class:compact>
  <span class="name" title={character.name}>{character.name}</span>
  {#if !compact}
    {#if character.active_title}
      <span class="title">{$titleName(character.active_title)}</span>
    {/if}
    <span class="meta">
      {$t('stat.level')}
      {character.level} · {$t(`class.${character.class}`)}
    </span>
    <span class="vitals">
      <span>{$t('stat.maxHp')} {character.max_hp}</span>
      <span>{$t('stat.maxMp')} {maxMp}</span>
    </span>
    <span class="stats">
      {#each attributes as attribute (attribute)}
        <span class="stat">
          <span>{$t(`stat.${attribute}`)}</span>
          <strong>{character.attributes[attribute]}</strong>
        </span>
      {/each}
    </span>
  {/if}
</span>

<style>
  .character-summary {
    display: grid;
    min-width: 0;
    gap: 5px;
    text-align: center;
    line-height: 1.3;
  }

  .name {
    overflow: hidden;
    color: #f7fafc;
    font-size: 16px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    color: #d6bcfa;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .meta,
  .vitals {
    color: #f0c040;
    font-size: 14px;
  }

  .vitals {
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: 2px 10px;
  }

  .vitals > span {
    white-space: nowrap;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 4px 12px;
    margin-top: 3px;
  }

  .stat {
    display: flex;
    justify-content: space-between;
    gap: 4px;
    color: #a7b7ca;
    font-size: 14px;
  }

  .stat strong {
    color: #e4ecf5;
    font-weight: 600;
  }

  @media (max-width: 600px), (max-height: 700px) {
    .name {
      font-size: 14px;
    }

    .stats {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }
</style>
