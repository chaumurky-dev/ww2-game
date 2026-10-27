"""Stage 1: build territory polygons (+ decor land) from Natural Earth, wrapped on a 10E-centred world."""
import json, os, pickle, sys
import shapely
from shapely import affinity
from shapely.geometry import box, Point, Polygon, MultiPolygon
from shapely.ops import unary_union
import ne

HERE = os.path.dirname(os.path.abspath(__file__))
CENTER = 10.0
LON_MIN = CENTER - 180.0     # -170
LON_MAX = CENTER + 180.0     #  190
LAT_MAX = 80.0
LAT_MIN = -65.3
WORLD = box(LON_MIN, LAT_MIN, LON_MAX, LAT_MAX)
SIMPLIFY_TOL = float(os.environ.get("TOL", 0.2))        # degrees; coverage-preserving, so shared borders stay shared
MIN_AREA = 0.3            # islands below this (deg2) leave the clickable territory ...
MIN_DECOR_AREA = 0.04     # ... and become purely visual decor if they are at least this big
THIN_TOL = float(os.environ.get("THIN", 0.02))   # degrees; DP thinning of smoothed runs (about 0.06 px)
CHAIKIN_ITER = int(os.environ.get("ITER", 2))          # corner-cutting passes over coastline runs (land-land borders are never touched)


def wrap_lon(lon):
    return lon + 360.0 if lon < LON_MIN else lon


def wrap_geom(g):
    """Re-centre a lon/lat geometry (in -180..180) on CENTER by cutting at LON_MIN."""
    if g.is_empty:
        return g
    east = g.intersection(box(LON_MIN, -90, 180.0001, 90))
    west = g.intersection(box(-180.0001, -90, LON_MIN, 90))
    if not west.is_empty:
        west = affinity.translate(west, xoff=360.0)
    return unary_union([east, west]) if not west.is_empty else east


def polys(g):
    if g.is_empty:
        return []
    if g.geom_type == "Polygon":
        return [g]
    if g.geom_type in ("MultiPolygon", "GeometryCollection"):
        out = []
        for p in g.geoms:
            out += polys(p)
        return out
    return []


def clean(g):
    g = shapely.make_valid(g) if not g.is_valid else g
    return unary_union(polys(g))


def parse_unit(u):
    """'RUS' | 'RUS:Moskva' | 'MYS@x0,y0,x1,y1' | 'JPN@~x0,y0,x1,y1' -> (a3, adm1 or None, bbox or None, invert)"""
    if ":" in u:
        a, n = u.split(":", 1)
        return a, n, None, False
    if "@" in u:
        a, b = u.split("@", 1)
        inv = b.startswith("~")
        bb = [float(x) for x in b.lstrip("~").split(",")]
        return a, None, bb, inv
    return u, None, None, False


def build(verbose=True):
    regions = json.load(open(os.path.join(HERE, "territory_regions.json")))
    patches = regions.pop("_patches", [])
    countries = ne.load("ne_50m_admin_0_countries")
    admin1 = ne.load("ne_50m_admin_1_states_provinces")
    c0 = {}
    for at, g in countries:
        a3 = at["ADM0_A3"]
        c0[a3] = clean(g) if a3 not in c0 else unary_union([c0[a3], clean(g)])
    # a3 aliases between datasets
    a1 = {}
    for at, g in admin1:
        a1.setdefault(at["adm0_a3"], {})[at["name"]] = clean(g)

    # claims[a3] = list of (territory or None, geom-in-lonlat)
    unit_terr = {}      # (a3, adm1) -> territory
    bare = {}           # a3 -> territory with bare code
    bbox_claims = {}    # a3 -> [(terr, bbox, invert)]
    for tid, spec in regions.items():
        for u in spec.get("units", []):
            a3, n, bb, inv = parse_unit(u)
            if n:
                if n not in a1.get(a3, {}):
                    raise SystemExit(f"unknown admin-1 {u} for {tid}")
                unit_terr[(a3, n)] = tid
            elif bb:
                bbox_claims.setdefault(a3, []).append((tid, bb, inv))
            else:
                if a3 not in c0:
                    raise SystemExit(f"unknown country {u} for {tid}")
                bare[a3] = tid

    parts = {tid: [] for tid in regions}
    parts["__decor__"] = []
    unclaimed_a1 = []

    for a3, geom in c0.items():
        geom_w = wrap_geom(geom)
        if a3 in a1 and any(k[0] == a3 for k in unit_terr):
            pieces = []
            for n, g in a1[a3].items():
                tid = unit_terr.get((a3, n), bare.get(a3, "__decor__"))
                if (a3, n) not in unit_terr:
                    unclaimed_a1.append((a3, n, tid))
                gg = wrap_geom(g).intersection(geom_w)
                pieces.append((tid, gg))
            covered = unary_union([p for _, p in pieces])
            left = clean(geom_w.difference(covered))
            for lp in polys(left):
                if lp.area < 1e-9:
                    continue
                best = min(pieces, key=lambda tp: tp[1].distance(lp))
                pieces.append((best[0], lp))
            for tid, g in pieces:
                parts[tid].append(g)
        elif a3 in bbox_claims:
            rest = geom_w
            for tid, bb, inv in bbox_claims[a3]:
                bx = box(*bb)
                bx = box(wrap_lon(bb[0]), bb[1], wrap_lon(bb[2]), bb[3])
                if inv:
                    sel = geom_w.difference(bx)
                else:
                    sel = geom_w.intersection(bx)
                parts[tid].append(sel)
                rest = rest.difference(sel)
            parts[bare.get(a3, "__decor__")].append(rest)
        else:
            parts[bare.get(a3, "__decor__")].append(geom_w)

    if verbose and unclaimed_a1:
        print("admin-1 units not explicitly claimed (went to bare/decor):")
        for x in unclaimed_a1:
            print("   ", x)

    terr = {}
    for tid, lst in parts.items():
        g = clean(unary_union(lst)) if lst else Polygon()
        spec = regions.get(tid, {})
        discs = [Point(wrap_lon(x), y).buffer(r, 12) for x, y, r in spec.get("discs", [])]
        if discs:
            g = unary_union([g] + discs)
        if spec.get("buffer"):
            g = g.buffer(spec["buffer"], 6)
        terr[tid] = g
    # carve discs out of other territories (gibraltar etc.), priority to discs
    for tid, spec in regions.items():
        if spec.get("discs") or spec.get("buffer"):
            for other in terr:
                if other != tid:
                    terr[other] = terr[other].difference(terr[tid])
    for pt in patches:
        poly = Polygon([(wrap_lon(x), y) for x, y in pt["poly"]])
        src = terr[pt["from"]] if pt.get("from") else unary_union(list(terr.values()))
        reg = poly.intersection(src).difference(terr[pt["to"]])
        for o in terr:
            if o != pt["to"]:
                terr[o] = terr[o].difference(reg)
        terr[pt["to"]] = clean(unary_union([terr[pt["to"]], reg]))
    for k in terr:
        terr[k] = clean(terr[k].intersection(WORLD))
    # small islands leave the clickable territory (keep tiny territories whole) and become decor
    extra_decor = []
    for k, g in terr.items():
        ps = polys(g)
        if not ps:
            continue
        if k == "__decor__" or g.area > 4.0:
            big = [p for p in ps if p.area >= MIN_AREA]
            if not big:
                big = [max(ps, key=lambda p: p.area)]
            if k != "__decor__":
                extra_decor += [p for p in ps if p.area < MIN_AREA and p.area >= MIN_DECOR_AREA
                                and not any(p is b for b in big)]
            else:
                big = [p for p in ps if p.area >= MIN_DECOR_AREA] or big
            terr[k] = unary_union(big)
    if extra_decor:
        terr["__decor__"] = unary_union([terr.get("__decor__", Polygon())] + extra_decor)
    return terr


def _key(c):
    return (round(c[0], 7), round(c[1], 7))


def _chaikin_open(pts, n):
    """Chaikin corner cutting on an open polyline; the two end points stay fixed."""
    for _ in range(n):
        if len(pts) < 3:
            break
        out = [pts[0]]
        for a, b in zip(pts, pts[1:]):
            out.append((0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]))
            out.append((0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]))
        out.append(pts[-1])
        pts = out
    if n and len(pts) > 2:
        # corner cutting multiplies the node count; drop nodes that no longer change the curve
        pts = [tuple(c) for c in shapely.LineString(pts).simplify(THIN_TOL, preserve_topology=False).coords]
    return pts


def _chaikin_closed(pts, n):
    for _ in range(n):
        out = []
        m = len(pts)
        for i in range(m):
            a, b = pts[i], pts[(i + 1) % m]
            out.append((0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]))
            out.append((0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]))
        pts = out
    if n and len(pts) > 6:
        thin = shapely.Polygon(pts).simplify(THIN_TOL, preserve_topology=True)
        if not thin.is_empty and thin.geom_type == "Polygon":
            pts = [tuple(c) for c in thin.exterior.coords][:-1]
    return pts


def _smooth_ring(coords, shared, n):
    """Smooth a closed ring: runs of consecutive unshared edges are Chaikin-cut with fixed ends;
    shared edges (land-land borders, world frame) are copied verbatim, so neighbours still match."""
    pts = [tuple(c) for c in coords[:-1]]
    m = len(pts)
    pin = [shared(pts[i], pts[(i + 1) % m]) for i in range(m)]   # edge i = pts[i] -> pts[i+1]
    if not any(pin):
        return _chaikin_closed(pts, n) if m >= 3 else pts
    if all(pin):
        return pts
    start = next(i for i in range(m) if pin[i])      # rotate so edge `start` (shared) is first
    pts = pts[start:] + pts[:start]
    pin = pin[start:] + pin[:start]
    out = []
    i = 0
    while i < m:
        if pin[i]:
            out.append(pts[i])
            i += 1
            continue
        j = i
        while j < m and not pin[j]:
            j += 1
        run = pts[i:j] + [pts[j % m]]
        out.extend(_chaikin_open(run, n)[:-1])
        i = j
    return out


def smooth_all(terr, n=CHAIKIN_ITER):
    """Chaikin smoothing of coastlines only. An edge is 'shared' when another geometry also has it
    (either direction) or when it lies on the world frame."""
    count = {}
    for g in terr.values():
        seen = set()
        for p in polys(g):
            for r in [p.exterior] + list(p.interiors):
                c = [_key(x) for x in r.coords]
                for a, b in zip(c, c[1:]):
                    seen.add((a, b) if a <= b else (b, a))
        for e in seen:
            count[e] = count.get(e, 0) + 1
    eps = 1e-6

    def shared(a, b):
        e = (_key(a), _key(b))
        e = e if e[0] <= e[1] else (e[1], e[0])
        if count.get(e, 0) > 1:
            return True
        for v in (LON_MIN, LON_MAX):
            if abs(a[0] - v) < eps and abs(b[0] - v) < eps:
                return True
        for v in (LAT_MIN, LAT_MAX):
            if abs(a[1] - v) < eps and abs(b[1] - v) < eps:
                return True
        return False

    out = {}
    for k, g in terr.items():
        res = []
        for p in polys(g):
            ext = _smooth_ring(list(p.exterior.coords), shared, n)
            holes = [_smooth_ring(list(i.coords), shared, n) for i in p.interiors]
            q = Polygon(ext, holes)
            res.append(q if q.is_valid else shapely.make_valid(q))
        out[k] = clean(unary_union(res)) if res else g
    return out


def resolve_overlaps(terr):
    """Smoothing can nudge two coastlines across a sliver gap left by the source data; carve any such
    overlap out of the larger geometry (decor always yields), so the result is again a disjoint coverage.
    Run in projected space: nodes made by an overlay in lon/lat would not lie on the projected neighbour edge."""
    keys = [k for k in terr if not terr[k].is_empty]
    for k in keys:
        terr[k] = clean(terr[k])
    tree = shapely.STRtree([terr[k] for k in keys])
    rank = lambda k: (0 if k == "__decor__" else 1, -terr[k].area)   # yields first = decor, then biggest
    for i, k in enumerate(keys):
        for j in tree.query(terr[k]):
            o = keys[j]
            if o == k or rank(k) > rank(o):
                continue            # handle each pair once, from the side that yields
            if shapely.intersection(terr[k], terr[o], grid_size=1e-6).area > 1e-9:
                terr[k] = clean(shapely.difference(terr[k], terr[o], grid_size=1e-6))
    return terr


def simplify_all(terr, tol=SIMPLIFY_TOL):
    keys = [k for k in terr if not terr[k].is_empty]
    geoms = [terr[k] for k in keys]
    simp = shapely.coverage_simplify(geoms, tol)
    out = {}
    for k, g in zip(keys, simp):
        out[k] = clean(g)
    return smooth_all(out)


if __name__ == "__main__":
    t = build()
    s = simplify_all(t)
    pickle.dump(s, open(os.path.join(HERE, "data", "land.pkl"), "wb"))
    for k, g in sorted(s.items()):
        print(k, round(g.area, 1), len(polys(g)))
