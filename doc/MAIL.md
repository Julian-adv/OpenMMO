# Mail

Players send letters to a character by name and read their own letters at any mailbox (#205). Letters are for arranging meetings and prices; items and gold do not travel by mail.

## Mailboxes

- Village: catalog entry `mailbox` (`data/object_catalog.json`, `"mailbox": true`, model `objects/mailbox.glb`). Place it with the map editor; point the slot side (+Z) south or west. Clicking it walks the player up and opens the mailbox panel.
- Estate: item `furniture_mailbox` (1,500c, ORKEA; display 115 by the south wall) uses the `estate_storage` placement path with model `mailbox`. Click it from within 3 m. Anyone can use an estate mailbox, not just the owner.
- Every mailbox is the same post office: a letter sent at one is read at any other. Nothing is stored per mailbox.
- The model is built by `tools/blender-scripts/build_mailbox.py`; its footprint is measured into `data/furniture_footprints.json` and must match `data-src/estate_storage.csv`.

## Letters

- Sending spends one Letter Paper (`letter`, 100c), sold by Aldwin; merchant sales burn the gold, so the price is the postage. The paper is returned if the letter is refused or the write fails.
- Recipient is a character name, matched ignoring ASCII case. Body up to 1,000 characters, no title.
- Each character may send once every 10 seconds. Muted players cannot send.
- A recipient keeps at most 100 letters; when full, new letters are refused until some are deleted.
- The inbox lists the 100 newest letters with sender, game date and the first 60 characters. Opening a letter marks it read. Deleting sets `deleted_at`; the row stays for moderation.

## Notices

- On entering the game the server sends `MailNotice { unread }` when the character has unread letters; the client shows it as an announcement toast.
- When a letter arrives for a character who is online, they get the same notice.

## Storage

`mail` table in the game database (`server/src/auth_mail.rs`): sender id and name at sending, recipient id, body, real and game time, `read_at`, `deleted_at`. Indexed on `(recipient_id, deleted_at, id)`. Letters sent more than 60 real days ago, read or not, are deleted when the server starts, so every deploy clears them.

## Protocol (v142)

Client: `OpenMailbox`, `ReadMail`, `SendMail`, `DeleteMail`, each with `mailbox` = `{ Village: { object_id } }` or `{ Estate: { furniture_id } }`.
Server: `Mailbox` (inbox), `MailLetter`, `MailSent`, `MailRefused`, `MailNotice`.

## Agents

agent-client lists the nearest mailbox within 30 m and offers `open_mailbox`, `read_letter`, `send_letter` (recipient, body) and `delete_letter`; each walks to the nearest mailbox first, and a letter's body is checked before the walk. The prompt shows the newest 10 letters and an open letter clipped to 400 characters. `MailNotice` becomes a `[Mail]` event and `MailRefused` a `[MailFailed]` event. Agents need Letter Paper like players.

