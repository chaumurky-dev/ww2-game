# mapgen: board geometry for Wartide

Generates `crates/web/assets/map_geometry.json` (SVG paths for every land territory and sea zone) from
Natural Earth data, matching the topology in `crates/engine/data/map.json`. The geometry is original:
it is derived only from public-domain Natural Earth boundaries, grouped by our own region definitions
and a hand-placed Voronoi partition of the oceans.

## Run

    cd scripts/mapgen
    ./fetch.sh                                   # downloads NE 1:50m admin_0 + admin_1 into data/ (gitignored)
    python3 -m venv .venv && .venv/bin/pip install shapely pyproj pyogrio numpy
    .venv/bin/python build.py --rebuild          # writes map_geometry.json, preview.svg, MISMATCHES.md

`check.py` prints the adjacency diff only; `where.py A:B` shows where two locations touch (debugging seeds).
`--rebuild` re-derives land from Natural Earth (about 20 s); without it `data/land.pkl` is reused, so seed tweaks are fast.

## Inputs you edit
- `label_overrides.json` / `anchor_overrides.json`: hand-placed name-label points and unit-stack anchors (`id: [x, y]` in viewBox units), applied after the automatic placement.
- `territory_regions.json`: per territory, Natural Earth units: `ISO3` (whole country), `ISO3:Admin1 name`,
  `ISO3@x0,y0,x1,y1` (country clipped to a lon/lat box), `ISO3@~x0,y0,x1,y1` (everything outside the box);
  optional `buffer` (degrees, to make tiny islands visible) and `discs` (synthetic islands: lon, lat, radius).
  Top-level `_patches` hand-carve lon/lat boxes from existing land into a territory (`to`, optional `from`) to fix contacts a grouping cannot (Wakhan, Altai corner, NW Ontario, Aqaba frontage, a thin no-man's-land strip between Morocco and Rio del Oro).
  Unlisted land becomes neutral `decor` (Greenland, Iceland, ...), emitted as top-level `decor` path.
- `sea_zone_seeds.json`: lon/lat seed points per sea zone (a zone is the union of its seeds' Voronoi cells
  clipped to water, computed in projected pixel space); `void` seeds mark Arctic water belonging to no zone;
  `virtual_links` are declared canal/strait adjacencies that need no touching water.

## Pipeline
1. Countries/admin-1 -> territories (re-centred on 10E; the Pacific is split across the edges, so a sea zone may be a MultiPolygon).
2. Coverage-preserving simplification (`shapely.coverage_simplify`, `SIMPLIFY_TOL` 0.2 deg) so shared borders stay shared; then Chaikin corner cutting (2 passes, thinned at 0.02 deg) on coastline runs only, i.e. edges no other geometry shares, so land-land borders are untouched; any overlap the smoothing creates is carved out in projected space. Islands < 0.3 deg2 leave their territory and, if >= 0.04 deg2, go to the purely visual `decor` layer. `TOL`, `ITER`, `THIN` env vars override the three parameters for experiments.
3. Miller projection into a 2000x1000 viewBox (lat -65.3..80).
4. Water = frame minus land; Voronoi cells from seeds; land/sea and sea/sea contact graph (min 8 px shared boundary, wrap-aware) vs `map.json`.
5. Output JSON, `preview.svg`, `MISMATCHES.md`.

Output schema: `{"view_box","projection","locations":{id:{kind,path,label,anchor,bbox}},"decor"}`.
`label` = pole of inaccessibility of the largest part; `anchor` = unit-stack position (label shifted 12 px down when it stays inside).

## Data license
Natural Earth (https://www.naturalearthdata.com) is in the public domain; no attribution required.
