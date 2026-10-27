"""Miller cylindrical projection into a 2000x1000 viewBox (centre meridian 10E)."""
import math
from geom import LON_MIN, LON_MAX, LAT_MAX, LAT_MIN, polys

W, H = 2000.0, 1000.0
def _my(lat):
    p = math.radians(lat)
    return 1.25 * math.log(math.tan(math.pi / 4 + 0.4 * p))
_Y0, _Y1 = _my(LAT_MAX), _my(LAT_MIN)
_SX = W / (LON_MAX - LON_MIN)
_SY = H / (_Y0 - _Y1)

def project(lon, lat):
    return ((lon - LON_MIN) * _SX, (_Y0 - _my(lat)) * _SY)

def project_geom(g):
    from shapely.ops import transform
    import numpy as np
    def f(x, y, z=None):
        x = np.asarray(x); y = np.asarray(y)
        p = np.radians(y)
        my = 1.25 * np.log(np.tan(np.pi / 4 + 0.4 * p))
        return ((x - LON_MIN) * _SX, (_Y0 - my) * _SY)
    return transform(f, g)

def path_d(g, nd=1):
    """SVG path (absolute, implicit lineto) for a (Multi)Polygon in projected coords."""
    def ring(r):
        cs = []
        for x, y in list(r.coords)[:-1]:
            c = (round(x, nd), round(y, nd))
            if not cs or cs[-1] != c:      # rounding collapses many near-duplicate nodes
                cs.append(c)
        if len(cs) > 1 and cs[0] == cs[-1]:
            cs.pop()
        return "M" + " ".join(f"{x:g} {y:g}" for x, y in cs) + "Z"

    out = []
    for p in polys(g):
        out.append(ring(p.exterior))
        for i in p.interiors:
            out.append(ring(i))
    return "".join(out)
