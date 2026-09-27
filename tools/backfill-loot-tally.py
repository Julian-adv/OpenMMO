#!/usr/bin/env python3
"""Turn server journal lines into loot tally upserts; usage in doc/METRICS.md (희귀 아이템 드롭).

--until must stop before the server that records the tally itself started, or kills count twice.
"""
import re
import sys
from collections import Counter
from datetime import datetime

ANSI = re.compile(r"\x1b\[[0-9;]*m")
STAMP = re.compile(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.\d+)?Z ")
KILL = re.compile(r"Player .+ killed (\S+) \(lvl (\d+)\) at (\(\S+\) [^;]+);")
DROPS = re.compile(r"Bonus drops \[(.*)\] at (\(\S+\) .+)$")
# Kill loot spawns after the swing's impact delay.
KILL_LOOT_WINDOW_SECONDS = 10


def hour(seconds):
    return seconds - seconds % 3600


def main():
    kills = Counter()
    drops = Counter()
    recent_kills = {}
    for raw in sys.stdin:
        line = ANSI.sub("", raw.rstrip("\n"))
        stamp = STAMP.match(line)
        if not stamp:
            continue
        seconds = int(datetime.fromisoformat(stamp.group(1) + "+00:00").timestamp())
        if kill := KILL.search(line):
            kills[(hour(seconds), kill.group(1), int(kill.group(2)))] += 1
            recent_kills[kill.group(3)] = seconds
        elif bonus := DROPS.search(line):
            killed_at = recent_kills.get(bonus.group(2).strip())
            source = "kill" if killed_at is not None and seconds - killed_at <= KILL_LOOT_WINDOW_SECONDS else "other"
            for item in re.findall(r'"([^"]+)"', bonus.group(1)):
                drops[(hour(seconds), item, source)] += 1

    print("BEGIN;")
    for (timestamp, monster_type, level), count in sorted(kills.items()):
        print(
            "INSERT INTO monster_kill_samples (timestamp, monster_type, level, kills) "
            f"VALUES ({timestamp}, '{monster_type}', {level}, {count}) "
            "ON CONFLICT(timestamp, monster_type, level) DO UPDATE SET kills = kills + excluded.kills;"
        )
    for (timestamp, item, source), count in sorted(drops.items()):
        print(
            "INSERT INTO item_drop_samples (item_def_id, source, timestamp, quantity) "
            f"VALUES ('{item}', '{source}', {timestamp}, {count}) "
            "ON CONFLICT(item_def_id, source, timestamp) DO UPDATE SET quantity = quantity + excluded.quantity;"
        )
    print("COMMIT;")
    print(f"-- kills {sum(kills.values())}, drops {sum(drops.values())}", file=sys.stderr)


if __name__ == "__main__":
    main()
