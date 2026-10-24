"""Stage 2: Voronoi sea zones over the water (computed in projected pixel space) + adjacency verification."""
import json, os
import numpy as np
import shapely
from shapely.geometry import box, MultiPoint, Point
from shapely.ops import unary_union
from shapely.affinity import translate
import proj
from geom import polys, wrap_lon

HERE = os.path.dirname(os.path.abspath(__file__))
FRAME = box(0, 0, proj.W, proj.H)


def load_seeds():
    return json.load(open(os.path.join(HERE, "sea_zone_seeds.json")))


def build_sea(land_px, seeds):
    """land_px: dict id -> projected geometry. Returns dict zone -> geometry (void excluded) and the water geom."""
    land_union = unary_union(list(land_px.values()))
    water = FRAME.difference(land_union)
    pts, names = [], []
    for z, lst in seeds["zones"].items():
        for lon, lat in lst:
            pts.append(proj.project(wrap_lon(lon), lat)); names.append(z)
    for lon, lat in seeds.get("void", []):
        pts.append(proj.project(wrap_lon(lon), lat)); names.append("__void__")
    mp = MultiPoint(pts)
    cells = shapely.voronoi_polygons(mp, extend_to=FRAME)
    zone_parts = {}
    # map each cell to its seed
    tree = shapely.STRtree([Point(p) for p in pts])
    for cell in cells.geoms:
        idx = tree.query(cell, predicate="contains")
        if len(idx) == 0:
            idx = tree.query(cell, predicate="intersects")
        i = int(idx[0])
        zone_parts.setdefault(names[i], []).append(cell.intersection(water))
    zones = {}
    for z, lst in zone_parts.items():
        g = unary_union(lst)
        ps = [p for p in polys(g) if p.area > 3.0]
        zones[z] = unary_union(ps) if ps else g.__class__()
    return zones, water


def contact_len(a, b, tol=0.05):
    """Length of shared boundary between polygonal a and b."""
    inter = a.boundary.intersection(b.buffer(tol))
    return inter.length


def graph(land_px, zones, min_contact=3.0):
    """Return found adjacencies: dict of frozenset({a,b}) -> length. Land-sea and sea-sea (wrap aware), land-land."""
    found = {}
    zid = [z for z in zones if z != "__void__" and not zones[z].is_empty]
    ltree = shapely.STRtree([land_px[k] for k in land_px])
    lkeys = list(land_px)
    for z in zid:
        zg = zones[z]
        for i in ltree.query(zg.buffer(1.0)):
            k = lkeys[int(i)]
            if k == "__decor__":
                continue
            L = contact_len(land_px[k], zg)
            if L >= min_contact:
                found[frozenset((k, z))] = L
    ztree = shapely.STRtree([zones[z] for z in zid])
    for i, z in enumerate(zid):
        for dx in (0.0, proj.W, -proj.W):
            zg = translate(zones[z], xoff=dx) if dx else zones[z]
            for j in ztree.query(zg.buffer(1.0)):
                z2 = zid[int(j)]
                if z2 <= z and dx == 0.0:
                    continue
                if z2 == z:
                    continue
                L = contact_len(zg, zones[z2])
                if L >= min_contact:
                    key = frozenset((z, z2))
                    found[key] = max(found.get(key, 0), L)
    # land-land
    keys = [k for k in lkeys if k != "__decor__"]
    for i, a in enumerate(keys):
        for j in ltree.query(land_px[a].buffer(1.0)):
            b = lkeys[int(j)]
            if b == "__decor__" or b <= a:
                continue
            L = contact_len(land_px[a], land_px[b], tol=0.6)
            if L >= min_contact:
                found[frozenset((a, b))] = L
    return found


def expected_graph(mapjson, land_ids, sea_ids):
    exp = set()
    for t in mapjson["territories"]:
        for n in t["neighbors"]:
            exp.add(frozenset((t["id"], n)))
    for s in mapjson["sea_zones"]:
        for n in s["neighbors"]:
            exp.add(frozenset((s["id"], n)))
    return exp


def compare(found, exp, land_ids, sea_ids, virtual):
    def kind(e):
        a, b = sorted(e)
        la, lb = a in land_ids, b in land_ids
        return "land-land" if la and lb else ("sea-sea" if not la and not lb else "land-sea")
    virt = {frozenset(v) for v in virtual}
    res = {"missing": [], "extra": []}
    for e in exp:
        if e not in found and e not in virt:
            res["missing"].append((kind(e), tuple(sorted(e))))
    for e, L in found.items():
        if e not in exp:
            res["extra"].append((kind(e), tuple(sorted(e)), round(L, 1)))
    return res
