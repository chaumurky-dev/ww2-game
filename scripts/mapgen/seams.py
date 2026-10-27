#!/usr/bin/env python3
"""Add `seam` ([[y0, y1], ...]) to every location in map_geometry.json.

A seam segment is a run of consecutive path vertices lying on the viewBox's west or east
edge (x ~ 0 or x ~ W). The web client paints these over with the location's fill so the
stroke of the clipped edge does not show where the three world copies meet.
Operates on the committed paths, so all other geometry stays byte-identical.
"""
import json, re, sys, pathlib

F = pathlib.Path(__file__).resolve().parents[2] / "crates/web/assets/map_geometry.json"
EPS = 0.15


def seams(path, w):
    out = []
    for ring in re.split(r"[Zz]", path):
        nums = [float(t) for t in re.findall(r"-?\d+(?:\.\d+)?", ring)]
        pts = list(zip(nums[0::2], nums[1::2]))
        if len(pts) < 2:
            continue
        on = [abs(x) < EPS or abs(x - w) < EPS for x, _ in pts]
        side = [0 if abs(x) < EPS else 1 for x, _ in pts]
        n = len(pts)
        for i in range(n):  # closed ring: edge i -> i+1
            j = (i + 1) % n
            if on[i] and on[j] and side[i] == side[j]:
                a, b = sorted((pts[i][1], pts[j][1]))
                if b - a > 0.05:
                    out.append([a, b])
    out.sort()
    merged = []
    for a, b in out:
        if merged and a <= merged[-1][1] + 0.3:
            merged[-1][1] = max(merged[-1][1], b)
        else:
            merged.append([a, b])
    return [[round(a, 1), round(b, 1)] for a, b in merged]


g = json.load(open(F))
w = float(g["view_box"].split()[2])
for lid, l in g["locations"].items():
    l.pop("seam", None)
    s = seams(l["path"], w)
    if s:
        l["seam"] = s
        print(lid, s, file=sys.stderr)
json.dump(g, open(F, "w"), separators=(",", ":"))
