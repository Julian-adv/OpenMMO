#!/usr/bin/env python3
"""Where top sellers got what they sold, and weapon-drop luck per killer.

Usage: item_sources.py server-*.txt  (normalized journal text, one file per run)
Values use the median merchant sale price of each item in the window.
"""
import re,sys,collections,statistics as st
SELL=re.compile(r"trading: (\S+) sold (?:(\d+)x)?(\S+) to (\S+) for (\d+)")
BUY=re.compile(r"trading: (\S+) bought (?:back )?(?:(\d+)x)?(\S+) from (\S+) for (\d+)")
PICK=re.compile(r'action="pickup" .*?name="([^"]*)" .*?item_def_id="([^"]+)" enchant=(\d+) quantity=(\d+) .*?dropped_by=(\S+)')
DROP=re.compile(r'action="drop" .*?name="([^"]*)" .*?item_def_id="([^"]+)" enchant=(\d+) quantity=(\d+)')
KILL=re.compile(r"combat: Player (\S+) killed (\S+) \(lvl (\d+)\)")
CHEST=re.compile(r"Player (\S+) opened dungeon chest '([^']*)': \[(.*?)\]")
TRADE=re.compile(r"Trade: (\S+) gave (.*?) \+ (\d+) copper to (\S+), who gave (.*?) \+ (\d+) copper")
sold=collections.defaultdict(collections.Counter); soldv=collections.Counter(); price=collections.defaultdict(list)
pick=collections.defaultdict(collections.Counter); pickp=collections.defaultdict(collections.Counter)
bought=collections.defaultdict(collections.Counter); kills=collections.Counter(); chests=collections.defaultdict(collections.Counter)
tradein=collections.defaultdict(collections.Counter); dropped=collections.defaultdict(collections.Counter)
for f in sys.argv[1:]:
    for l in open(f):
        if 'trading:' in l:
            m=SELL.search(l)
            if m:
                q=int(m[2] or 1); v=int(m[5]); sold[m[1]][m[3]]+=q; soldv[m[1]]+=v; price[m[3]].append(v/q); continue
            m=BUY.search(l)
            if m: bought[m[1]][m[3]]+=int(m[2] or 1); continue
        elif 'item_audit' in l:
            m=PICK.search(l)
            if m:
                (pick if m[5]=='None' else pickp)[m[1]][m[2]]+=int(m[4]); continue
            m=DROP.search(l)
            if m: dropped[m[1]][m[2]]+=int(m[4])
        elif ' killed ' in l:
            m=KILL.search(l)
            if m: kills[m[1]]+=1
        elif 'dungeon chest' in l:
            m=CHEST.search(l)
            if m:
                for it in re.findall(r'"([^"]+)"',m[3]): chests[m[1]][it]+=1
        elif 'Trade:' in l:
            m=TRADE.search(l)
            if m:
                for side,who in ((m[2],m[4]),(m[5],m[1])):
                    for q,it in re.findall(r'(?:(\d+)x)?([a-z_0-9]+)',side):
                        if it and it!='-': tradein[who][it]+=int(q or 1)
P={k:st.median(v) for k,v in price.items()}
val=lambda c:sum(P.get(k,0)*q for k,q in c.items())
rows=[]
for n,v in soldv.items():
    if v<100000: continue
    src_m=val(pick[n]); src_p=val(pickp[n]); src_c=val(chests[n]); src_t=val(tradein[n]); src_b=val(bought[n])
    rows.append((v,n,kills[n],src_m,src_c,src_p,src_t,src_b))
rows.sort(reverse=True)
print(f"{'name':12} {'sold':>9} {'kills':>6} {'sold/kill':>9} | value of items obtained: {'ground(mob)':>11} {'chest':>8} {'from players':>12} {'trades in':>9} {'bought':>8}")
for v,n,k,a,b,c,d,e in rows[:25]:
    print(f"{n[:12]:12} {v:>9,} {k:>6} {v/max(k,1):>9,.0f} | {a:>11,.0f} {b:>8,.0f} {c:>12,.0f} {d:>9,.0f} {e:>8,.0f}")
spk=[r[0]/r[2] for r in rows if r[2]>=100]
m,sd,md=st.mean(spk),st.pstdev(spk),st.median(spk)
print(f"\nsold/kill among sellers with >=100 kills: n={len(spk)} mean={m:,.0f} sd={sd:,.0f} median={md:,.0f}")
for v,n,k,*_ in rows:
    if k>=100 and (v/k-m)/sd>2: print('  >2sd',n,round(v/k),round((v/k-m)/sd,1))

K=re.compile(r"combat: Player (\S+) killed (\S+) \(lvl \d+\) .*?; weapon drop: (\S+)")
by=collections.defaultdict(collections.Counter); seen=collections.Counter(); dropped_n=collections.Counter()
for f in sys.argv[1:]:
    for l in open(f):
        m='; weapon drop:' in l and K.search(l)
        if m:
            d=m[3]!='none'; by[m[1]][(m[2],d)]+=1; seen[m[2]]+=1; dropped_n[m[2]]+=d
rate={k:dropped_n[k]/seen[k] for k in seen}
luck=[]
for p,c in by.items():
    n=sum(c.values())
    if n<200: continue
    got=sum(q for (mt,d),q in c.items() if d); exp=sum(q*rate[mt] for (mt,d),q in c.items())
    luck.append(((got-exp)/max((exp*(1-exp/n))**0.5,1),p,n,got,exp))
luck.sort(reverse=True)
print("\nweapon drops vs expected for the same monsters (z):")
for z,p,n,got,exp in luck[:8]: print(f"  {p[:12]:12} kills={n} drops={got} expected={exp:.0f} z={z:.1f}")
