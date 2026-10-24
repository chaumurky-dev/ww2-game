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
        cs = list(r.coords)[:-1]
        s = "M" + " ".join(f"{round(x, nd):g} {round(y, nd):g}" for x, y in cs) + "Z"
        return s
    out = []
    for p in polys(g):
        out.append(ring(p.exterior))
        for i in p.interiors:
            out.append(ring(i))
    return "".join(out)
