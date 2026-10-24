# wartide-web

Leptos 0.8 (CSR) frontend for Wartide, built with [trunk](https://trunkrs.dev).

## Run

```sh
# toolchain: rustup target add wasm32-unknown-unknown; cargo install trunk --locked
cd crates/web
trunk serve          # http://127.0.0.1:8081, proxies /api and /ws to localhost:8080
trunk build --release
cargo check -p wartide-web --target wasm32-unknown-unknown
cargo clippy -p wartide-web --target wasm32-unknown-unknown -- -D warnings
```

Note: with Homebrew's `rust` shadowing rustup, put `~/.rustup/toolchains/stable-*/bin` first on `PATH`
so the wasm target is found.

## Layout

- `src/lib.rs` - `App`, router (`/`, `/verify`, `/forgot`, `/reset`, `/lobby`, `/game/:id`,
  `/profile/:username`, `/groups`, `/groups/:slug`, `/leaderboard`, `/settings`).
- `src/api.rs` - the only place that talks to the backend (REST helpers, WS subscribe). Currently returns mock
  data (`USE_MOCK = true`). Each function carries a `TODO(protocol)` marker with the endpoint it should call.
- `src/mock/` - local stand-ins for protocol DTOs (`Power`, `UnitKind`, `GameSummary`, ...) and canned data.
  Replace with `wartide-protocol` types and delete.
- `src/geometry.rs` - loads `/assets/map_geometry.json` (falls back to `map_geometry.placeholder.json`).
  Accepted schema: `{ "viewBox": [x,y,w,h], "locations": [ { "id", "name", "kind": "land"|"sea", "path",
  "anchor": [x,y], "label": [x,y], "ipc" } ] }`. Missing anchors/labels are approximated from the path.
- `src/components/board.rs` - `Board` SVG component and the `BoardModel` view-model (territory owners, unit stacks,
  highlights, selection). Fill it from the engine `GameState`; location keys are geometry ids.
  `viewport.rs` holds pan/zoom maths (pointer drag, pinch, wheel, buttons).
- `src/components/` - nav, toasts, modal, countdown, power badge, unit icon (PNG mask with SVG glyph fallback),
  phase bar, action panel (per-phase), battle board, chat drawer/log, form fields.
- `src/pages/` - one module per route.
- `styles/app.css` - all styling, CSS variables for power colours / sea tints (optional `data-theme="dark"`).

## Mocked

Auth, lobby, matchmaking, game state, actions, dice, chat, profiles, groups, leaderboards. The game page has a
"Mock phase" selector to preview each phase panel; any action advances the phase stepper locally.
Unit icons: drop PNGs at `assets/units/<id>.png` (`infantry`, `armor`, `fighter`, `bomber`, `battleship`,
`carrier`, `transport`, `submarine`, `aa_gun`, `industrial_complex`); until then an inline glyph is shown.
