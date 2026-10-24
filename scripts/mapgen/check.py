"""Run stages 1-2 and print adjacency mismatches."""
import json, os, pickle, sys
import proj, sea
from shapely.ops import unary_union
HERE = os.path.dirname(os.path.abspath(__file__))
MAP = os.path.join(HERE, "..", "..", "crates", "engine", "data", "map.json")

def run(rebuild=False, quiet=False):
    p = os.path.join(HERE, "data", "land.pkl")
    if rebuild or not os.path.exists(p):
        import geom
        pickle.dump(geom.simplify_all(geom.build(False)), open(p, "wb"))
    land = pickle.load(open(p, "rb"))
    land_px = {k: proj.project_geom(g) for k, g in land.items()}
    seeds = sea.load_seeds()
    zones, water = sea.build_sea(land_px, seeds)
    m = json.load(open(MAP))
    lid = {t["id"] for t in m["territories"]}; sid = {s["id"] for s in m["sea_zones"]}
    found = sea.graph(land_px, zones)
    exp = sea.expected_graph(m, lid, sid)
    res = sea.compare(found, exp, lid, sid, seeds.get("virtual_links", []))
    return land_px, zones, water, m, found, exp, res

if __name__ == "__main__":
    land_px, zones, water, m, found, exp, res = run(rebuild="--rebuild" in sys.argv)
    for kind in ("land-sea", "sea-sea", "land-land"):
        mi = [x for x in res["missing"] if x[0] == kind]; ex = [x for x in res["extra"] if x[0] == kind]
        print(f"== {kind}: missing {len(mi)}, extra {len(ex)}")
        if kind == "land-land" and "--ll" not in sys.argv:
            continue
        for x in sorted(mi): print("  MISSING", *x[1])
        for x in sorted(ex): print("  EXTRA  ", *x[1], x[2])
    print("zones without geometry:", [z for z in {s['id'] for s in m['sea_zones']} if z not in zones or zones[z].is_empty])
