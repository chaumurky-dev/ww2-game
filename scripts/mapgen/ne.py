"""Minimal Natural Earth loader (no geopandas): returns list of (attrs dict, shapely geom)."""
import os
import pyogrio
import shapely
HERE = os.path.dirname(os.path.abspath(__file__))

def load(name):
    meta, _, geoms, fields = pyogrio.raw.read(os.path.join(HERE, "data", name + ".shp"))
    names = list(meta["fields"])
    out = []
    for i, g in enumerate(shapely.from_wkb(geoms)):
        out.append(({n: fields[j][i] for j, n in enumerate(names)}, g))
    return out
