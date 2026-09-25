#!/usr/bin/env python3
"""Heroic-tale candidates (doc/HEROIC_TALES.md) from openmmo-server journal
lines on stdin. Prints natural-language ledger lines for a human to review and
append to agent-client/data/tales/ledger.txt — never appends itself.

Usage: journalctl -u openmmo-server --since "<KST>" -o cat | python3 tales.py [DATE]
DATE defaults to today (UTC). Optional: --min-break N (default 7).
Place names come from ~/work/OnlineRPG/data/map_labels.json (the prod checkout).
"""
import collections
import datetime
import json
import math
import os
import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")
LEVEL = re.compile(r"Player (\S+) reached level (\d+)")
KILL = re.compile(r"Player (\S+) killed (\S+) \(lvl \d+\) at \((-?[\d.]+),(-?[\d.]+)\) (.*?);")
DIED = re.compile(r"Player (\S+) died to (\S+) at \((-?[\d.]+),(-?[\d.]+)\) (.*)")
ENCHANT = re.compile(r"(\S+) enchanted (\S+) to \+(\d+)")
BREAK = re.compile(r"(\S+) destroyed (\S+) enchanting at \+(\d+)")
TITLE = re.compile(r"Player (\S+) earned title '([^']+)'")
TITLE_OFFLINE = re.compile(r"Character (\d+) earned title '([^']+)'")

BOSSES = {"goblin_boss", "orc_boss", "ogre_boss"}
DATA_DIR = os.path.expanduser("~/work/OnlineRPG/data")


def load_data(filename):
    try:
        with open(os.path.join(DATA_DIR, filename)) as f:
            return json.load(f)
    except OSError:
        return {}


def names(data):
    return {key: value["name"] for key, value in data.items() if "name" in value}


def display(registry, value):
    return registry.get(value, value.replace("_", " "))


MONSTER_NAMES = names(load_data("monsters.json"))
ITEM_NAMES = names(load_data("items.json"))
TITLE_NAMES = names(load_data("titles.json"))
LABEL_DATA = load_data("map_labels.json")
LABEL_NAMES = names(LABEL_DATA)
LABELS = {
    key: (value["x"], value["z"])
    for key, value in LABEL_DATA.items()
    if value["kind"] != "continent"
}
ALDERMARK = LABELS.get("aldermark", (-1475.2, 4741.6))

args = [a for a in sys.argv[1:] if not a.startswith("--")]
date = args[0] if args else datetime.datetime.utcnow().strftime("%Y-%m-%d")
min_break = 7
if "--min-break" in sys.argv:
    min_break = int(sys.argv[sys.argv.index("--min-break") + 1])

levels = collections.defaultdict(set)
boss_kills = []
boss_deaths = []
enchants = []
breaks = []
titles = []
offline_titles = []
farthest = {}


def note_pos(name, x, z):
    d = math.hypot(x - ALDERMARK[0], z - ALDERMARK[1])
    if d > farthest.get(name, (0, 0, 0))[0]:
        farthest[name] = (d, x, z)


for raw in sys.stdin:
    line = ANSI.sub("", raw).rstrip()
    m = LEVEL.search(line)
    if m:
        levels[m[1]].add(int(m[2]))
        continue
    m = KILL.search(line)
    if m:
        note_pos(m[1], float(m[3]), float(m[4]))
        if m[2] in BOSSES:
            boss_kills.append((m[1], m[2], m[5]))
        continue
    m = DIED.search(line)
    if m:
        note_pos(m[1], float(m[3]), float(m[4]))
        if m[2] in BOSSES:
            boss_deaths.append((m[1], m[2], m[5]))
        continue
    m = ENCHANT.search(line)
    if m:
        enchants.append((int(m[3]), m[1], m[2]))
        continue
    m = BREAK.search(line)
    if m:
        breaks.append((int(m[3]), m[1], m[2]))
        continue
    m = TITLE.search(line)
    if m:
        titles.append((m[1], m[2]))
        continue
    m = TITLE_OFFLINE.search(line)
    if m:
        offline_titles.append((m[1], m[2]))


def out(name, brief):
    print(f"# REVIEW {date} | {name} | {brief}")


print("# candidates — check solo/first against the ledger and DB before appending")
for name, boss, place in boss_kills:
    out(
        name,
        f"{name} defeated the {display(MONSTER_NAMES, boss)} in {place}. Before appending, state "
        "whether it was solo or the server first only if verified, and add the "
        "intended celebratory performance direction.",
    )
for name, boss, place in boss_deaths:
    out(
        name,
        f"{name} fell to the {display(MONSTER_NAMES, boss)} in {place}. Sing the loss as a tragedy, "
        f"never ridicule {name}.",
    )
for name, title in titles:
    out(name, f'{name} earned the title "{display(TITLE_NAMES, title)}". Celebrate the achievement.')
for cid, title in offline_titles:
    out(
        f"character#{cid}",
        f'Character #{cid} earned the title "{display(TITLE_NAMES, title)}". Resolve the current '
        "character name in the DB before appending, then celebrate the achievement.",
    )
if enchants:
    plus, name, item = max(enchants)
    out(
        name,
        f"{name} enchanted a {display(ITEM_NAMES, item)} to +{plus}, the audit window's highest. "
        "Compare with the DB before calling it a realm record, then state the "
        "intended celebratory performance direction.",
    )
for plus, name, item in sorted(breaks, reverse=True):
    if plus >= min_break:
        out(
            name,
            f"{name}'s {display(ITEM_NAMES, item)} shattered while enchanting past +{plus}. "
            f"Sing it as a tragedy and do not ridicule {name}.",
        )
if levels:
    name = max(levels, key=lambda n: len(levels[n]))
    top = max(levels[name])
    gained = len(levels[name])
    out(
        name,
        f"{name} climbed {gained} levels during the audit window, more than anyone "
        f"observed, and reached level {top}. Celebrate the climb.",
    )
    print("# highest level: compare", name, "at", top, "with the DB maximum")
if farthest:
    name, (d, x, z) = max(farthest.items(), key=lambda kv: kv[1][0])
    near = min(LABELS, key=lambda k: math.hypot(x - LABELS[k][0], z - LABELS[k][1]), default="?")
    out(
        name,
        f"{name} travelled farther from Aldermark than anyone observed, reaching "
        f"coordinates ({x:.0f}, {z:.0f}) near {display(LABEL_NAMES, near)}, about {int(d)} metres "
        "away. Celebrate the journey without inventing places or encounters.",
    )
print("# daily experience leader: diff this audit's (name, level, xp) snapshot against the previous one")
