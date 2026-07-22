# How to write announcements

Add one `.md` file per announcement to this directory and it shows up on the
login screen. Entries are sorted newest-first by date and time, and the newest
50 are shown. Restart the server after adding or editing an announcement.

## File format

Name files like `YYYY-MM-DD-title.md`. The leading date is used for sorting.

```
---
title: 던전 시스템 업데이트
title_en: Dungeon System Update
date: 2026-07-21
time: 14:30
category: update
---
한국어 본문을 여기에 작성합니다.
줄바꿈은 그대로 표시됩니다.
[en]
Write the English body here.
Line breaks are preserved as-is.
```

- `title`: Default-language (Korean) title. If omitted, the first body line (or
  the first `# heading`) is used.
- `title_en`: English title. Add other languages as `title_<code>` (e.g. `title_ja`).
- `date`: If omitted, the `YYYY-MM-DD` prefix of the filename is used. An entry
  with no resolvable date is left out.
- `time`: Optional `HH:MM` or `HH:MM:SS`, used only for ordering — it is never
  displayed. Add it when multiple announcements share a date so the latest one
  appears first. A malformed value is ignored, and entries with no time sort
  below timed ones on the same date.
- `category`: Optional. Shown as a tag on the login screen (e.g. update, plan, event).
- Body: Text before the first `[xx]` marker is the default language (Korean). A
  line like `[en]` starts that language's body.

Put multiple languages in one file and users can switch with the language buttons
on the login screen. Write only one language (no `[xx]` markers) to show just that
one. Files whose name starts with `_` (including this README) are excluded from the list.
