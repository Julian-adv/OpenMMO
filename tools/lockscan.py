#!/usr/bin/env python3
"""Static lock-order scanner for tokio RwLock/Mutex guards in server/src.

Finds guards held across `.await` points, follows calls transitively, and
reports held->acquired edges plus cycles (ABBA inversions). Heuristic: it
misses locks taken through free functions or nested fields and tracks guard
scopes by indentation. Lock order is documented above `GameState` in
server/src/game_state/mod.rs.

    python3 -I tools/lockscan.py server/src            # report
    python3 -I tools/lockscan.py --check server/src    # exit 1 on any cycle
"""
import argparse
import os
import re
import sys
from collections import defaultdict

ap = argparse.ArgumentParser()
ap.add_argument('root')
ap.add_argument('fns', nargs='?', help='comma-separated fns: print transitive locks')
ap.add_argument('edge_fns', nargs='?', help='comma-separated fns: print edges located in them')
ap.add_argument('--check', action='store_true',
                help='exit 1 when a cycle is found, or when nothing was scanned')
args = ap.parse_args()
ROOT = args.root

files = []
for d, _, fs in os.walk(ROOT):
    for f in fs:
        if f.endswith('.rs') and f != 'tests.rs' and 'tests' not in d:
            files.append(os.path.join(d, f))
files.sort()

FN_RE = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*[<(]')
LOCK_RE = re.compile(r'(?:self|game|game_state|state|gs)\.(\w+)\s*\.\s*(read|write|lock)\(\)\s*\.await')
MLOCK_RE = re.compile(r'\.(lock_player_movement|world_edit_guard|lock_player_persistence|lock_character_sessions|reserve_workers)\(.*?\.await|\.(goal_moves|movement_regions)\.lock\(.*?\.await')


def find_locks(ln):
    out = list(LOCK_RE.findall(ln))
    for m in MLOCK_RE.finditer(ln):
        name = m.group(1) or m.group(2)
        op = 'lock'
        if name == 'movement_regions':
            op = 'write' if 'true)' in m.group(0) else 'read'
        out.append((name, op))
    return out


LET_RE = re.compile(r'^\s*let\s+(?:mut\s+)?(\w+)\s*(?::[^=]+)?=\s*(?:self|game|game_state|state|gs)\.(\w+)\s*\.\s*(read|write|lock)\(\)\s*\.await\s*;')
CALL_RE = re.compile(r'(?:self|game|game_state|state|gs)\.(\w+)\s*\(')
DROP_RE = re.compile(r'drop\((\w+)\)')


def strip_tests(src):
    i = src.find('#[cfg(test)]\nmod ')
    return src if i < 0 else src[:i]


def paren_depth(t):
    t = re.sub(r'"(?:\\.|[^"\\])*"', '""', t)
    return t.count('(') - t.count(')')


fns = {}
for path in files:
    with open(path, encoding='utf-8') as fh:
        src = strip_tests(fh.read())
    lines = src.split('\n')
    i = 0
    while i < len(lines):
        m = FN_RE.match(lines[i])
        if not m or lines[i].rstrip().endswith(';'):
            i += 1
            continue
        name = m.group(1)
        depth = 0
        started = False
        j = i
        while j < len(lines):
            for ch in lines[j]:
                if ch == '{':
                    depth += 1
                    started = True
                elif ch == '}':
                    depth -= 1
            if started and depth == 0:
                break
            j += 1
        body = lines[i:j + 1]
        # fold multi-line statements so a lock and its `.await` share a line
        joined = []
        for ln in body:
            s = ln.strip()
            if joined and (s.startswith('.') or joined[-1].rstrip().endswith('=') or paren_depth(joined[-1]) > 0):
                joined[-1] = joined[-1].rstrip() + ('' if s.startswith('.') else ' ') + s
            else:
                joined.append(ln)
        fns.setdefault(name, []).append((path, i + 1, joined))
        i = j + 1

direct = defaultdict(set)
calls = defaultdict(set)
for name, defs in fns.items():
    for path, start, body in defs:
        for ln in body:
            for f, op in find_locks(ln):
                direct[name].add((f, op))
            for c in CALL_RE.findall(ln):
                if c in fns:
                    calls[name].add(c)

trans = {n: set(direct[n]) for n in fns}
changed = True
while changed:
    changed = False
    for n in fns:
        before = len(trans[n])
        for c in calls[n]:
            trans[n] |= trans[c]
        if len(trans[n]) != before:
            changed = True

edges = defaultdict(list)
HELD_RE = re.compile(r'^\s*let\s+(?:Some\(\s*)?(?:mut\s+)?(\w+)\)?\s*=\s*(?:match\s+)?(?:self|game|game_state|state|gs)(.*\.await)\s*(?:\?\s*;|;|else\s*\{|\{)\s*$')
for name, defs in fns.items():
    for path, start, body in defs:
        held = []
        for k, ln in enumerate(body):
            indent = len(ln) - len(ln.lstrip())
            if ln.strip().startswith('}'):
                held = [h for h in held if h[3] <= indent]
            for v in DROP_RE.findall(ln):
                held = [h for h in held if h[0] != v]
            if '.await' in ln and held:
                acquired = set()
                for f, op in find_locks(ln):
                    acquired.add((f, op, 'direct'))
                for c in CALL_RE.findall(ln):
                    if c in fns:
                        for f, op in trans[c]:
                            acquired.add((f, op, c))
                for h in held:
                    for f, op, via in acquired:
                        loc = f"{os.path.relpath(path, ROOT)}:{start + k} in {name}" + ("" if via == 'direct' else f" via {via}")
                        edges[(h[1], h[2], f, op)].append(loc)
            m = LET_RE.match(ln)
            m2 = HELD_RE.match(ln)
            if m:
                held.append((m.group(1), m.group(2), m.group(3), indent, k))
            elif m2 and find_locks(ln):
                for f, op in find_locks(ln):
                    held.append((m2.group(1), f, op, indent, k))
            elif ln.rstrip().endswith('{') and re.match(r'^\s*(for|if let|while let|match)\b', ln):
                for f, op in find_locks(ln):
                    held.append(('<tmp>', f, op, indent, k))

g = defaultdict(set)
for (a, aop, b, bop) in edges:
    if a != b:
        g[a].add(b)

cycles = set()


def dfs(startn, node, pathn):
    for nxt in g[node]:
        if nxt == startn and len(pathn) > 1:
            cyc = tuple(pathn)
            mi = cyc.index(min(cyc))
            cycles.add(cyc[mi:] + cyc[:mi])
        elif nxt not in pathn and len(pathn) < 4:
            dfs(startn, nxt, pathn + [nxt])


for n in list(g):
    dfs(n, n, [n])

print("== lock-order edges (held -> acquired) ==")
for (a, aop, b, bop), locs in sorted(edges.items()):
    if a == b:
        continue
    print(f"{a}.{aop} -> {b}.{bop}  [{len(locs)}]  e.g. {locs[0]}")
print("\n== same-lock re-acquire while held (self-deadlock on tokio locks) ==")
for (a, aop, b, bop), locs in sorted(edges.items()):
    if a == b:
        for l in locs:
            print(f"{a}.{aop} -> {b}.{bop}  {l}")
print("\n== cycles ==")
for c in sorted(cycles, key=len):
    print(' -> '.join(c) + ' -> ' + c[0])
    for i in range(len(c)):
        a, b = c[i], c[(i + 1) % len(c)]
        for (x, xop, y, yop), locs in edges.items():
            if x == a and y == b:
                print(f"    {a}.{xop} -> {b}.{yop}: " + '; '.join(locs[:3]))

if args.fns:
    print("\n== transitive locks for requested fns ==")
    for n in args.fns.split(','):
        print(n, sorted(trans.get(n, set())))
        print("   calls:", sorted(calls.get(n, set())))

if args.edge_fns:
    print("\n== all held->acquired edges located in requested fns ==")
    want = args.edge_fns.split(',')
    for (a, aop, b, bop), locs in sorted(edges.items()):
        for l in locs:
            if any(f" in {w}" in l for w in want):
                print(f"{a}.{aop} -> {b}.{bop}   {l}")

n_edges = sum(1 for (a, _, b, _) in edges if a != b)
print(f"\nlockscan: {len(files)} files, {n_edges} edges, {len(cycles)} cycles")
if args.check:
    if not files or not n_edges:
        print("lockscan: nothing scanned; wrong root?", file=sys.stderr)
        sys.exit(1)
    if cycles:
        print("lockscan: lock-order cycle found; see the documented order above GameState in server/src/game_state/mod.rs", file=sys.stderr)
        sys.exit(1)
