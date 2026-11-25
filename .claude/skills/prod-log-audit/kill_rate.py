#!/usr/bin/env python3
"""Kill speed per character and per dungeon floor.

Usage: kill_rate.py server-*.txt  (normalized journal text, one file per run)
Compare against the server limits: basic attack interval (player_anim_timing.csv
player_attack_interval, or the weapon attackIntervalMs), surface ambient spawns
(ambient_spawn.rs SPAWN_CHANCE_PER_METER per metre walked, world.json
maxNearbyMonsters), dungeon slot respawn (dungeon.rs MONSTER_RESPAWN_MS_BY_PLAYER_COUNT).
"""
import re,sys,collections,statistics as st,datetime,bisect
KILL=re.compile(r"combat: Player (\S+) killed (\S+) \(lvl (\d+)\) at \(([-\d.]+),([-\d.]+)\) (surface|dungeon '([^']+)' depth (\d+))")
LVL=re.compile(r"Player (\S+) reached level (\d+)")
ABIL=re.compile(r"ability cast name=(\S+) ability=(\S+) targets=(\d+)")
def ts(l): return datetime.datetime.strptime('2026-'+l[:21],'%Y-%m-%d %H:%M:%S.%f').timestamp()
kills=collections.defaultdict(list); lvl={}; spawns=collections.defaultdict(list); same_sec=collections.Counter()
for f in sys.argv[1:]:
    run=f
    for l in open(f):
        if ' killed ' in l:
            m=KILL.search(l)
            if m: kills[m[1]].append((ts(l),m[2],int(m[3]),m[7] or 'surface'))
        elif 'reached level' in l:
            m=LVL.search(l)
            if m: lvl[m[1]]=int(m[2])
rows=[]
for n,ks in kills.items():
    if len(ks)<200: continue
    t=[k[0] for k in ks]; t.sort()
    mins=len({int(x//60) for x in t})
    mx60=max(bisect.bisect_right(t,x+60)-i for i,x in enumerate(t))
    mx600=max(bisect.bisect_right(t,x+600)-i for i,x in enumerate(t))
    gaps=[b-a for a,b in zip(t,t[1:])]
    fast=sum(1 for g in gaps if g<1.0)
    dun=sum(1 for k in ks if k[3]!='surface')/len(ks)
    rows.append(dict(n=n,k=len(ks),mins=mins,kpm=len(ks)/mins,mx60=mx60,mx600=mx600,fast=fast/len(gaps),dun=dun,lvl=lvl.get(n,0),mlvl=st.median(k[2] for k in ks)))
for key in ('kpm','mx60','mx600'):
    v=[r[key] for r in rows]; m,sd=st.mean(v),st.pstdev(v)
    print(f"== {key}: n={len(v)} mean={m:.2f} sd={sd:.2f} median={st.median(v):.2f}")
    for r in sorted(rows,key=lambda r:-r[key])[:8]:
        print(f"  {r['n'][:12]:12} kills={r['k']:5} active_min={r['mins']:4} kpm={r['kpm']:.2f} max60s={r['mx60']:3} max10m={r['mx600']:4} <1s_gaps={r['fast']:.0%} dungeon={r['dun']:.0%} median_mob_lvl={r['mlvl']} z={(r[key]-m)/sd:.1f}")
# Dungeon floors: kills of non-boss monsters per floor in any 120 s window.
# A slot respawns no sooner than 120 s (5+ players) and resets only at night.
DK=re.compile(r"combat: Player (\S+) killed (\S+) \(lvl \d+\) .*?dungeon '([^']+)' depth (\d+)")
floors=collections.defaultdict(list); killers=collections.defaultdict(set)
for f in sys.argv[1:]:
    for l in open(f):
        if "dungeon '" in l and ' killed ' in l:
            m=DK.search(l)
            if m and 'boss' not in m[2]:
                floors[(f,m[3],int(m[4]))].append(ts(l)); killers[(f,m[3],int(m[4]))].add(m[1])
print("\n== dungeon floors: max kills in 120 s, total, distinct killers")
out=[]
for k,t in floors.items():
    t.sort()
    if len(t)>=20: out.append((max(bisect.bisect_right(t,x+120)-i for i,x in enumerate(t)),k[1],k[2],len(t),len(killers[k])))
for r in sorted(out,reverse=True)[:10]: print("  ",r)
