# House scroll templates

These files are immutable source snapshots for the five village house scrolls.
They are embedded in the server at build time and must not be read from mutable
runtime housing data.

| Template | Snapshot source |
|---|---|
| `rica-shop.json` | `data/housing/r-23_+73/r-23_+73_1.json` |
| `karl-house.json` | `data/housing/r-23_+73/r-23_+73_2.json` |
| `aldwin-house.json` | `data/housing/r-23_+74/r-23_+74_3.json` |
| `aldermark-inn.json` | `data/housing/r-23_+74/r-23_+74_2.json` |
| `rowan-house.json` | `data/housing/r-23_+74/r-23_+74_4.json` |

Captured on 2026-09-07. Replace a template deliberately when changing the
corresponding scroll; runtime edits never update it automatically.
