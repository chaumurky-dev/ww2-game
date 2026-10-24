"""Debug: where.py A:B [C:D ...] -> pixel/lon location of contact between two ids."""
import sys, check, sea, proj
land_px, zones, *_ = check.run()
g = {**land_px, **zones}
for arg in sys.argv[1:]:
    x, y = arg.split(":")
    inter = g[x].boundary.intersection(g[y].buffer(0.3))
    print("##", arg)
    for p in getattr(inter, "geoms", [inter]):
        if p.length > 3:
            c = p.centroid
            print(f"   len {p.length:.1f} px ({c.x:.0f},{c.y:.0f}) lon~{c.x / proj.W * 360 - 170:.1f}")
