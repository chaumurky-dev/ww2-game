"""Write preview.svg (and optionally PNG) from the pipeline state."""
import json, os, sys
import proj
from geom import polys
HERE = os.path.dirname(os.path.abspath(__file__))

def write_preview(land_px, zones, m, labels=None, seeds=None, path=None):
    path = path or os.path.join(HERE, "preview.svg")
    col = {p["id"]: p["color"] for p in m["powers"]}
    owner = {t["id"]: t.get("original_owner") for t in m["territories"]}
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2000 1000" font-family="sans-serif">',
           '<rect width="2000" height="1000" fill="#dbe9f4"/>']
    sea_ids = [s["id"] for s in m["sea_zones"]]
    blues = ["#bcd7ec", "#a9cbe6", "#c9e0f1", "#9fc2df"]
    for i, z in enumerate(sorted(sea_ids)):
        if z in zones and not zones[z].is_empty:
            out.append(f'<path d="{proj.path_d(zones[z])}" fill="{blues[i % 4]}" stroke="#6f9bbd" stroke-width="0.8" fill-rule="evenodd"/>')
    for k, g in land_px.items():
        if g.is_empty:
            continue
        if k == "__decor__":
            fill = "#e4e1d8"
        else:
            o = owner.get(k)
            fill = col.get(o, "#b9b9b9") if o else "#b9b9b9"
        out.append(f'<path d="{proj.path_d(g)}" fill="{fill}" fill-opacity="0.85" stroke="#333" stroke-width="0.7" fill-rule="evenodd"/>')
    if labels:
        for k, (x, y) in labels.items():
            short = k[3:] if k.startswith("sz_") else k
            if k.startswith("sz_"):
                num = k
                out.append(f'<text x="{x:.0f}" y="{y:.0f}" font-size="8" fill="#1d4f7a" text-anchor="middle">{short.replace("_"," ")}</text>')
            else:
                out.append(f'<text x="{x:.0f}" y="{y:.0f}" font-size="9" font-weight="bold" fill="#111" text-anchor="middle">{short.replace("_"," ")}</text>')
    if seeds:
        for z, lst in seeds["zones"].items():
            for lon, lat in lst:
                from geom import wrap_lon
                x, y = proj.project(wrap_lon(lon), lat)
                out.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="1.6" fill="#c00"/>')
    out.append("</svg>")
    open(path, "w").write("\n".join(out))
    return path
