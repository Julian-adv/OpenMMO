#!/usr/bin/env python3
"""Rename characters whose names collide ignoring ASCII case.

Within each colliding group the most progressed character (level, xp,
item count, then earliest creation) keeps the name; the rest get the
lowest free numeric suffix. Dry-run by default; pass --apply to write.
Run while the game server is stopped.
"""

import argparse
import sqlite3
import sys

MAX_NAME_CHARS = 32


def ascii_lower(name: str) -> str:
    return "".join(c.lower() if "A" <= c <= "Z" else c for c in name)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("db_path")
    parser.add_argument("--apply", action="store_true", help="write changes (default: dry-run)")
    args = parser.parse_args()

    conn = sqlite3.connect(args.db_path)
    rows = conn.execute(
        """SELECT c.id, c.character_name, c.level, c.xp, c.created_at,
                  (SELECT COUNT(*) FROM character_items i WHERE i.character_id = c.id)
           FROM characters c"""
    ).fetchall()

    taken = {ascii_lower(name) for _, name, *_ in rows}
    groups = {}
    for row in rows:
        groups.setdefault(ascii_lower(row[1]), []).append(row)

    renames = []
    for group in groups.values():
        if len(group) < 2:
            continue
        group.sort(key=lambda r: (-r[2], -r[3], -r[5], r[4], r[0]))
        keeper = group[0]
        for char_id, name, *_ in group[1:]:
            suffix = 2
            while True:
                candidate = f"{name}{suffix}"
                if len(candidate) <= MAX_NAME_CHARS and ascii_lower(candidate) not in taken:
                    break
                suffix += 1
            taken.add(ascii_lower(candidate))
            renames.append((char_id, name, candidate, keeper[1]))

    if not renames:
        print("No case-colliding names found.")
        return 0

    width = max(len(old) for _, old, _, _ in renames)
    for char_id, old, new, kept_by in renames:
        print(f"id {char_id:>5}  {old:<{width}} -> {new:<{width + 1}} (name kept by: {kept_by})")
    print(f"\n{len(renames)} characters renamed across {sum(1 for g in groups.values() if len(g) > 1)} groups.")

    if not args.apply:
        print("Dry-run only; re-run with --apply to write.")
        return 0

    has_blocks = conn.execute(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'character_blocks'"
    ).fetchone()
    with conn:
        for char_id, old, new, _ in renames:
            conn.execute("UPDATE characters SET character_name = ? WHERE id = ?", (new, char_id))
            if has_blocks:
                conn.execute(
                    "UPDATE character_blocks SET blocked_name = ? WHERE blocked_name = ?",
                    (new, old),
                )
    print("Applied.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
