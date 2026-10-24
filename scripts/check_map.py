#!/usr/bin/env python3
"""Validate crates/engine/data/map.json (and units.json): symmetry, references, incomes, setup."""
import json, sys, os, collections
root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "engine", "data")
m = json.load(open(os.path.join(root, "map.json")))
units = json.load(open(os.path.join(root, "units.json")))
errs = []
def err(s): errs.append(s)

land = {t["id"]: t for t in m["territories"]}
sea = {s["id"]: s for s in m["sea_zones"]}
if set(land) & set(sea): err("id collision between land and sea")
allp = {**land, **sea}
powers = {p["id"]: p for p in m["powers"]}
unit_ids = {u["id"] for u in units}

# adjacency: symmetric, resolvable, no self loops, no duplicates
for a, n in allp.items():
    nb = n["neighbors"]
    if len(nb) != len(set(nb)): err(f"{a}: duplicate neighbors")
    for b in nb:
        if b == a: err(f"{a}: self loop")
        elif b not in allp: err(f"{a}: unknown neighbor {b}")
        elif a not in allp[b]["neighbors"]: err(f"asymmetric: {a} -> {b}")
for s in sea.values():
    for b in s["neighbors"]:
        if b not in sea and b not in land: err(f"{s['id']}: bad neighbor {b}")
# every coastal land territory should touch sea or land; no isolated nodes
for a, n in allp.items():
    if not n["neighbors"]: err(f"{a}: isolated")
# connectivity (whole world graph)
seen = set(); stack = [next(iter(allp))]
while stack:
    x = stack.pop()
    if x in seen: continue
    seen.add(x); stack.extend(allp[x]["neighbors"])
if seen != set(allp): err(f"disconnected: {sorted(set(allp)-seen)}")

# incomes
want = {"ussr": 24, "germany": 32, "uk": 30, "japan": 25, "usa": 36}
inc = collections.Counter()
for t in land.values():
    if t["original_owner"]: inc[t["original_owner"]] += t["ipc"]
    if t["neutral"] and (t["original_owner"] or t["ipc"]): err(f"neutral {t['id']} has owner/ipc")
    if not t["neutral"] and not t["original_owner"]: err(f"{t['id']} non-neutral without owner")
for p, w in want.items():
    if inc[p] != w: err(f"income {p}: {inc[p]} != {w}")
    if powers[p]["starting_ipcs"] != w: err(f"starting_ipcs {p}")
    # starting treasury equals starting income in this edition
# capitals / complexes
caps = [t for t in land.values() if t["is_capital"]]
if len(caps) != 5: err(f"capital count {len(caps)}")
for p in powers.values():
    c = land.get(p["capital"])
    if not c: err(f"capital {p['capital']} missing")
    elif c["original_owner"] != p["id"] or not c["is_capital"]: err(f"capital mismatch {p['id']}")
    if c and not c["industrial_complex"]: err(f"capital {c['id']} lacks complex")
if sum(t["industrial_complex"] for t in land.values()) != 8: err("expected 8 original complexes")

# starting units
for u in m["starting_units"]:
    if u["location"] not in allp: err(f"start loc {u}")
    if u["owner"] not in powers: err(f"start owner {u}")
    if u["unit"] not in unit_ids: err(f"start unit {u}")
    udef = next((x for x in units if x["id"] == u["unit"]), None)
    if udef:
        if udef["domain"] == "sea" and u["location"] not in sea: err(f"ship on land {u}")
        if udef["domain"] == "land" and u["location"] not in land: err(f"land unit at sea {u}")
        if u["location"] in land and land[u["location"]]["original_owner"] != u["owner"]:
            err(f"unit not in own territory {u}")
ic = {u["location"] for u in m["starting_units"] if u["unit"] == "industrial_complex"}
if ic != {t["id"] for t in land.values() if t["industrial_complex"]}: err("complex flags vs units mismatch")
# fighters at sea must have carrier capacity
for loc in sea:
    f = sum(u["count"] for u in m["starting_units"] if u["location"] == loc and u["unit"] == "fighter")
    c = sum(u["count"] for u in m["starting_units"] if u["location"] == loc and u["unit"] == "carrier")
    if f > 2 * c: err(f"{loc}: fighters exceed carrier capacity")
for c in m.get("canals", []):
    for x in c["sea_zones"]:
        if x not in sea: err(f"canal sz {x}")
    for x in c["land_territories"]:
        if x not in land: err(f"canal land {x}")
    a, b = c["sea_zones"]
    if b not in sea[a]["neighbors"]: err(f"canal zones not adjacent {c['name']}")
# rulebook-stated adjacency facts
def adj(a, b): return b in allp[a]["neighbors"]
if adj("gibraltar", "algeria"): err("gibraltar-algeria must not be land adjacent")
if adj("eastern_europe", "turkey"): err("eastern_europe-turkey must not be land adjacent")
if adj("finland_norway", "eastern_europe"): err("finland_norway-eastern_europe must not be adjacent")
if not adj("anglo_egypt_sudan", "syria_iraq"): err("suez land adjacency")
if adj("kwangtung", "sz_japan"): err("kwangtung must not touch japan sea zone")
if not (adj("karelia_s_s_r", "sz_baltic") and adj("karelia_s_s_r", "sz_karelia")): err("karelia two sea zones")
if not adj("western_canada", "eastern_canada"): err("canada wraparound")
if not adj("western_usa", "eastern_usa"): err("usa wraparound")
if not adj("sz_gulf_of_mexico", "sz_mexico"): err("gulf-mexico sea zone wrap")
if not adj("mexico", "panama"): err("mexico-panama")
print(f"territories={len(land)} sea_zones={len(sea)} start_unit_rows={len(m['starting_units'])}")
print("incomes:", dict(inc))
if errs:
    print("FAIL"); [print(" -", e) for e in errs]; sys.exit(1)
print("OK")
