#!/usr/bin/env python3
"""Per-character gold and XP outliers across hourly prod DB backups.

Usage: outliers_sigma.py START END   (backup stamps, e.g. 20261120-120141 20261121-150008)
Reads ~/backups/OpenMMO/hourly/game_data-prod-*.db read-only. Only characters
whose gold or XP changed in some interval count. Prints mean/sd z and
median/MAD robust z; both matter because gold is heavy-tailed.
"""
import glob
import os
import re
import sqlite3
import statistics as st
import sys
from datetime import datetime

start, end = sys.argv[1], sys.argv[2]
files = sorted(glob.glob(os.path.expanduser("~/backups/OpenMMO/hourly/game_data-prod-*.db")))
snaps = []
for f in files:
    stamp = re.search(r"(\d{8}-\d{6})", f)[1]
    if not start <= stamp <= end:
        continue
    db = sqlite3.connect(f"file:{f}?mode=ro", uri=True)
    rows = {
        r[0]: r[1:]
        for r in db.execute(
            "select id, character_name, account_name, level, xp, gold from characters"
            " where account_name not like 'npc_%'"
        )
    }
    db.close()
    snaps.append((datetime.strptime(stamp, "%Y%m%d-%H%M%S"), rows))

stats = []
for cid in snaps[-1][1]:
    prev = None
    gold = xp = active = 0
    active_hours = best = 0.0
    for t, rows in snaps:
        cur = rows.get(cid)
        if cur is None:
            prev = None
            continue
        if prev:
            hours = (t - prev[0]).total_seconds() / 3600
            dx, dg = cur[3] - prev[1][3], cur[4] - prev[1][4]
            if dx or dg:
                active += 1
                active_hours += hours
            xp += dx
            gold += dg
            best = max(best, dx / hours)
        prev = (t, cur)
    if active:
        name, account, level = snaps[-1][1][cid][:3]
        stats.append(dict(name=name, account=account, level=level, gold=gold,
                          xp_per_hour=xp / active_hours, best_xp_per_hour=best, active=active))


def report(key):
    values = [s[key] for s in stats]
    mean, sd = st.mean(values), st.pstdev(values) or 1
    median = st.median(values)
    mad = st.median(abs(v - median) for v in values) or 1
    print(f"== {key}: n={len(values)} mean={mean:,.0f} sd={sd:,.0f} median={median:,.0f} MAD={mad:,.0f}")
    for s in sorted(stats, key=lambda s: -s[key])[:12]:
        z = (s[key] - mean) / sd
        robust = 0.6745 * (s[key] - median) / mad
        flag = " <<" if z > 2 or robust > 3.5 else ""
        print(f"  {s['name'][:12]:12} {s['account']:14} L{s['level']:<3} {s[key]:>13,.0f}"
              f" z={z:5.1f} robust={robust:6.1f} active={s['active']}{flag}")


print(f"snapshots {len(snaps)}: {snaps[0][0]} .. {snaps[-1][0]}")
for key in ("gold", "xp_per_hour", "best_xp_per_hour"):
    report(key)

print("== same 10-level band (z > 2)")
bands = {}
for s in stats:
    bands.setdefault(min(s["level"] // 10, 5), []).append(s)
for band, members in sorted(bands.items()):
    if len(members) < 4:
        continue
    for key in ("gold", "xp_per_hour"):
        values = [s[key] for s in members]
        mean, sd = st.mean(values), st.pstdev(values) or 1
        flagged = [f"{s['name']}({s[key]:,.0f} z={(s[key] - mean) / sd:.1f})"
                   for s in members if (s[key] - mean) / sd > 2]
        print(f"  L{band * 10}-{band * 10 + 9} n={len(members)} {key} mean={mean:,.0f} sd={sd:,.0f}: {', '.join(flagged) or '-'}")
