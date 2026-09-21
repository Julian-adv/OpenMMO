# Localization

The browser client uses i18next with a Svelte store. Supported languages are
English (`en`), Korean (`ko`), Japanese (`ja`), and Simplified Chinese (`zh-Hans`).
Traditional Chinese is not yet provided; `zh-TW`, `zh-HK`, `zh-MO`, and `zh-Hant`
fall through to the next supported browser language or English.

## Language selection

Settings → Language changes the UI without reloading the page or reconnecting.
The selection is stored as `onlinerpg_language` in local storage. Auto uses the
browser's ordered language preferences, with English as the final fallback.
Storage failures do not prevent switching languages. The HTML `lang` attribute
tracks the selected language.

Title Language retains its separate preference for existing users. Auto now
follows the game language. Announcements use the same game language, falling back
to their English or Korean content when a translation is unavailable.

## Adding UI messages

Catalogs live in `client/src/lib/i18n/locales/{locale}.json`. Use stable semantic
keys and translate complete sentences. Keep interpolation variable names identical
across languages. Svelte escapes the rendered text; do not render translations
with `{@html}`.

```svelte
<script lang="ts">
  import { t } from '../i18n'
</script>

<span>{$t('system.playerRevived', { name: playerName })}</span>
```

`$t` updates when the language changes. TypeScript modules can use
`translate(key, values)` for event-time text. Chat and combat history keeps the
language used when each line arrived. Player chat and player names are not
localization keys; the existing optional chat translation feature is separate.

## Game data

Keep IDs, stats, prices, and the English source in `data-src/*.csv`. Do not translate
IDs or edit generated `data/*.json` files. Item translations are separate
`{locale}.items.json` catalogs keyed by `<item ID>.name` and `.description`.

Use `itemDisplayName(id, enchant, $locale)` or
`itemDescription(def, $locale)` in Svelte. The explicit locale makes derived
labels update immediately. `getItemDef()` continues to return canonical game
data. Missing translations fall back to the English definition; unknown IDs
remain visible as IDs. Retain enchantment prefixes in every language.

The first migration includes all 141 current item names and descriptions, all
nine titles, settings, inventory and its dialogs/tooltips, revival, login/loading
labels, fishing/combat log helpers, and selected system/trade feedback. Other
panels may still contain English labels. NPC/monster/skill names, remaining HUD,
character creation, social/housing UI, and remaining server feedback should be
migrated using the same helpers. Translation text should receive native-speaker
review before a localized release.

## Server messages

Protocol **96** adds optional `localization` metadata to `SystemMessage` and
`TradeError`, retaining the English `message` and the original event kinds:

```json
{
  "message": "You picked up 12 copper.",
  "localization": {
    "code": "server.goldPickedUp",
    "params": { "amount": "12" }
  }
}
```

In the server, pass `localized(code, english).with_param(name, value)` to
`send_system_message()` or `send_trade_error()`. Supply the code at the action
that produced the message; do not infer it by matching English text. Plain
strings remain supported for incremental migration. Add the code and its
translations to all four UI catalogs.

The browser uses the code and variables, falling back to `message` for unknown
codes or absent metadata. The agent client continues to consume the English
message. MessagePack encodes fields positionally, so this change requires matching
server, rebuilt browser WASM, and rebuilt agent-client binaries. Do not deploy a
protocol-96 server with a protocol-95 browser or agent client.

## Validation

- `cd client && npm test -- src/lib/i18n/i18n.test.ts` checks language resolution,
  persistence, reactivity, catalog completeness, interpolation variables, and
  fallback behavior.
- `cd client && npm run check && npm run lint` checks UI integration.
- `cargo test -p onlinerpg-shared --test localized_messages` checks MessagePack
  round trips with and without metadata.
- Rebuild browser bindings with `cd client && npm run build:wasm` after protocol
  changes. Check all languages in settings, open item tooltips, and a narrow
  viewport; language changes must not reload the document.
