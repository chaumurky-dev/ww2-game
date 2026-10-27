#!/usr/bin/env python3
"""Compare crates/engine/data/map.json with the TripleA "World War II Classic" (2nd edition) setup XML.

Usage: python3 scripts/audit_map.py path/to/classic.xml
Source (triplea-maps org, repo world_war_ii_classic, file map/games/classic.xml):
  https://raw.githubusercontent.com/triplea-maps/world_war_ii_classic/master/map/games/classic.xml
Names are matched after normalisation (case, punctuation, a few known aliases); every name that
differs is reported so spelling variants are visible. Exit code 1 when any data discrepancy is found.
"""
import json, os, re, sys, collections
import xml.etree.ElementTree as ET

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "engine", "data")
m = json.load(open(os.path.join(ROOT, "map.json")))
x = ET.parse(sys.argv[1]).getroot()


def norm(s):
    s = s.lower().replace("mediteranean", "mediterranean").replace("carribean", "caribbean")
    s = s.replace("antartic", "antarctic").replace(".", "")
    return re.sub(r"[^a-z0-9]+", "", s)


ALIAS = {  # classic-xml name (normalised) -> our name (normalised)
    "anglosudanegypt": "angloegyptsudan", "eastcanada": "easterncanada", "easteurope": "easterneurope",
    "eastus": "easternusa", "southeurope": "southerneurope", "syriajordan": "syriairaq",
    "westcanada": "westerncanada", "westeurope": "westerneurope", "westus": "westernusa",
}
POWER = {"Russians": "ussr", "Germans": "germany", "British": "uk", "Japanese": "japan", "Americans": "usa"}
UNIT = {"armour": "armor", "aaGun": "aa_gun"}

ours = {}
for t in m["territories"]:
    ours[norm(t["name"])] = t["id"]
for s in m["sea_zones"]:
    ours[norm(s["name"])] = s["id"]


def ident(name, sea):
    n = norm(name)
    if not sea:
        n = ALIAS.get(n, n)
    return ours.get(n)


# Deliberate, documented deviations from the community file (docs/RULES.md "Interpretations" 14).
KNOWN_EXTRA_ADJ = {("sz_gulf_of_mexico", "sz_mexico")}
problems = []


def bad(msg):
    problems.append(msg)
    print("DIFF", msg)


land_x, sea_x = [], []
for t in x.find("map").findall("territory"):
    (sea_x if t.get("water") == "true" else land_x).append(t.get("name"))
idmap = {}
names = {t["id"]: t["name"] for t in m["territories"] + m["sea_zones"]}
for sea, lst in ((False, land_x), (True, sea_x)):
    for n in lst:
        i = ident(n, sea)
        if i is None:
            bad(f"classic territory missing from ours: {n}")
        else:
            idmap[n] = i
            if names[i] != n:
                print(f"NAME  classic '{n}' vs ours '{names[i]}'")
mapped = set(idmap.values())
for i in names:
    if i not in mapped:
        bad(f"ours not on classic board: {i}")

cl = collections.defaultdict(set)
for c in x.find("map").findall("connection"):
    a, b = idmap.get(c.get("t1")), idmap.get(c.get("t2"))
    if a and b:
        cl[a].add(b)
        cl[b].add(a)
allp = {t["id"]: t for t in m["territories"] + m["sea_zones"]}
for i, t in allp.items():
    mine = set(t["neighbors"])
    for b in sorted(cl[i] - mine):
        if i < b:
            bad(f"adjacency missing in ours: {i} - {b}")
    for b in sorted(mine - cl[i]):
        if i < b and (i, b) not in KNOWN_EXTRA_ADJ:
            bad(f"adjacency extra in ours: {i} - {b}")

prod, capital, factory = {}, {}, set()
for a in x.iter("attachment"):
    if a.get("name") != "territoryAttachment":
        continue
    n = idmap.get(a.get("attachTo"))
    for o in a.findall("option"):
        if o.get("name") == "production":
            prod[n] = int(o.get("value"))
        if o.get("name") == "capital":
            capital[n] = POWER[o.get("value")]
        if o.get("name") == "originalFactory":
            factory.add(n)
owner = {idmap[o.get("territory")]: POWER[o.get("owner")] for o in x.iter("territoryOwner")}
for t in m["territories"]:
    i = t["id"]
    if t["ipc"] != prod.get(i, 0):
        bad(f"{i}: ipc ours {t['ipc']} classic {prod.get(i, 0)}")
    if t["original_owner"] != owner.get(i):
        bad(f"{i}: owner ours {t['original_owner']} classic {owner.get(i)}")
    if t["is_capital"] != (i in capital):
        bad(f"{i}: capital flag ours {t['is_capital']} classic {i in capital}")
    if t["industrial_complex"] != (i in factory):
        bad(f"{i}: IC ours {t['industrial_complex']} classic {i in factory}")
    if t["neutral"] != (i not in owner):
        bad(f"{i}: neutral flag ours {t['neutral']} classic {i not in owner}")
    if t["impassable"]:
        bad(f"{i}: impassable set in ours; classic has no impassable territories")

cu = collections.Counter()
for u in x.iter("unitPlacement"):
    if u.get("unitType") == "factory":
        continue
    cu[(idmap[u.get("territory")], POWER[u.get("owner")], UNIT.get(u.get("unitType"), u.get("unitType")))] += int(u.get("quantity"))
ou = collections.Counter()
for u in m["starting_units"]:
    if u["unit"] == "industrial_complex":
        continue  # compared via the industrial_complex flag above
    ou[(u["location"], u["owner"], u["unit"])] += u["count"]
for k in sorted(set(cu) | set(ou)):
    if cu[k] != ou[k]:
        bad(f"units {k}: ours {ou[k]} classic {cu[k]}")

for a in x.iter("attachment"):
    if a.get("name") == "canalAttachment":
        print("CANAL", a.get("attachTo"), [o.get("value") for o in a.findall("option")])
print(f"\n{len(problems)} discrepancies")
sys.exit(1 if problems else 0)
