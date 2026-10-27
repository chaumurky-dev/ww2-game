# wartide-engine

Deterministic, pure (`std` + serde/serde_json/thiserror/rand_chacha) rules engine for Wartide.
No I/O, no clocks, no global RNG; compiles to `wasm32-unknown-unknown`. The server and AI call
`apply`; the web client uses the same crate for legality hints. Rules authority: `docs/RULES.md`;
data: `data/map.json`, `data/units.json` (embedded with `include_str!`).

## API overview

```rust
use wartide_engine::{apply, pending_decision, Action, GameSettings, GameState, Power};

let mut st = GameState::new(GameSettings::default());     // printed setup, USSR, round 1
let events = apply(&mut st, Power::Ussr, &Action::EndPhase, /*seed*/ 1)?;   // atomic: state only changes on Ok
let who = pending_decision(&st);                          // Decision::Phase / SelectCasualties / RetreatOrPress / SubWithdrawal / GameOver
```

* `apply(&mut GameState, actor, &Action, seed) -> Result<Vec<Event>, RuleError>`: validates everything,
  rolls dice from `ChaCha8Rng::seed_from_u64(seed)`, never panics on bad input, leaves the state untouched on `Err`.
  The server derives `seed` from `(game_seed, action_seq)`, so a game replays from its action log:
  `replay(settings, &[(Power, Action, u64)]) -> Result<GameState>`.
* `Action` (tagged `{"type": ...}`): `BuyResearch`, `Purchase`, `Move{units,path}`, `Load`, `Unload`, `UndoMoves{scope}`, `EndPhase`,
  `BeginBattle`, `DeclareStrategicBombing`, `SelectCasualties`, `Retreat`, `PressAttack`, `WithdrawSubs`,
  `FireRocket`, `Place`, `Concede`.
* `Event` (tagged): every die roll (`DiceRolled{power, context, rolls}`), moves, loads/unloads, AA fire,
  bombardment, casualties, captures/liberation/capital seizure, tech, income, phase and turn changes,
  neutral violation, battle start/round/end, game over.
* `pending_decision(&GameState) -> Decision`: who must act and what kind of input is needed.
* `query` module: `reachable(state, unit) -> Vec<Reach{to, path, battle, landing}>`, `load_options`,
  `unload_options`, `legal_placements`, `purchase_options`, `rocket_targets`, `bombing_targets`,
  `battles_pending`, `retreat_destinations`, `battle_odds` (Monte-Carlo with the real combat code),
  `income`, `production_level`.
* Additive query/event helpers (all pure, read-only):
  * `Event::DiceRolled` has `details: Vec<DieDetail{unit, unit_type, owner, hit_on, roll, hit}>`, parallel to
    `rolls`, for combat, first strike, bombardment, AA, research, strategic bombing and rocket dice. Research
    dice have `unit: None, hit_on: 6`; tech-selection, bombing-damage and rocket dice have `unit: None,
    hit_on: 0, hit: false` (damage/selection, no hit concept). AA dice name the gun. Old stored events
    without `details` deserialize to an empty vector (`#[serde(default)]`).
  * `query::reachable_group(state, &[UnitId]) -> Vec<Reach>`: destinations plus one path legal for the whole
    group, moving at the slowest unit's speed. Units must share owner (the active power), start square and
    domain (all land, all air or all sea); a mixed land/air group, duplicates or a foreign unit give `[]`.
    `landing` is true only if every unit is an aircraft that can land there.
  * `query::current_battle_odds(state, trials, seed) -> Option<BattleOdds>`: Monte-Carlo of the rest of the
    battle in `state.battle` (pending casualties resolved cheapest-first, the attacker presses every round),
    from the exact current units and step, via the real combat code. `None` without a battle.
    `battle_odds(state, loc, ..)` remains for battles that have not started.
  * `query::legal_placements_for(state, unit_type, count) -> Vec<Placement{location, from_complex,
    capacity_left}>`: every legal `Place` for `count` units of a purchased type. Honours per-complex capacity
    (`capacity_left` is the supplying complex's remaining allowance, `u32::MAX` = unlimited original complex),
    one AA gun per territory, the four-new-complex cap (new complexes: `from_complex: None`,
    `capacity_left` = new complexes still allowed) and, for ships, one entry per adjacent supplying complex
    (pass it as `Action::Place.from`). `legal_placements` (single unit, locations only) is unchanged.
  * `query::movable_units(state, power) -> Vec<UnitId>`: units of the active power with moves left and at least
    one reachable destination in the current movement phase (empty in other phases or for other powers).
  * `query::landing_status(state, unit) -> LandingStatus`: `Safe` (not an aircraft, or on a legal landing spot
    or carrier), `NeedsLanding{options}` (landing spots within the remaining moves, avoiding untouched
    neutrals and hostile sea) or `Stranded` (none in range: lost at the end of non-combat). Works in both
    move phases; serialized as `{"type": "safe" | "needs_landing" | "stranded"}`.
  * Undo movement: `Action::UndoMoves{scope}` (JSON `{"type":"undo_moves","scope":{"type":"all"}}`,
    `{"type":"units","units":[ids]}` or `{"type":"into","location":"<id>"}`) is legal for the active power in
    CombatMove / NonCombatMove. The engine keeps `turn.phase_start` (snapshot of units, `next_unit_id`,
    control, treasuries) and `turn.move_journal` (applied `Move`/`Load`/`Unload`, each with `aa_fired`) for
    the open movement phase only; both are cleared when the phase ends and absent from the JSON otherwise.
    Undo restores the snapshot and replays the unselected entries without events or dice; entries that
    depended on a removed one are dropped too. Emits one `Event::MovesUndone{power, units, cascaded}`
    (`UnitRef`s). If any move of the phase drew antiaircraft fire, every undo is `Illegal`.
    `query::moved_this_phase(state) -> Vec<(UnitId, from, to)>` lists units standing away from where the
    phase began; `query::undoable(state, &scope) -> bool` says whether the undo would succeed now.
    Bots and `RandomPolicy` never undo.
* `random_policy::RandomPolicy`: a random legal-move player (fuzzing, bot fallback).
* `MapData::get()`: static map: lookups by id/name, `neighbours`, `is_sea`, `ipc`, `original_owner`,
  `capital_of`, `canals`. `LocationId` is a small integer internally and its string id in JSON.

### Turn flow

`DevelopWeapons → Purchase → CombatMove → Combat → NonCombatMove → Mobilize → (CollectIncome, automatic)`.
Players press `EndPhase`, except: Combat ends by itself when nothing is left to resolve, and income, victory
checks and the hand-over to the next power happen inside the `EndPhase` of Mobilize.

### State JSON (abridged)

```json
{
  "settings": {"weapons_development": true, "axis_economic_victory": 84, "allied_economic_victory": null,
               "soviet_first_turn_restriction": false, "original_complexes_only": false,
               "total_victory": false, "germany_jet_power": false, "japan_super_subs": false, "seed": 0},
  "round": 1, "current_power": "ussr", "phase": "develop_weapons",
  "treasury": {"ussr": 24, "germany": 32, "uk": 30, "japan": 25, "usa": 36},
  "techs": {"ussr": [], "germany": [], "uk": [], "japan": [], "usa": []},
  "control": {"russia": "ussr", "germany": "germany", "...": "..."},
  "units": [{"id": 1, "kind": "infantry", "owner": "ussr", "location": "caucasus", "born_turn": 0}],
  "next_unit_id": 162, "purchases": [], "new_complexes_placed": 0,
  "turn": {"number": 1, "start_control": {"...": "..."}, "resolved": [], "researched": false,
           "rocket_fired": false, "placed": []},
  "battle": null, "conceded": [], "winner": null
}
```

Cargo is expressed by `"loaded_on": <transport/carrier id>` on the carried unit. Per-turn flags
(`moves_used`, `frozen`, `combat_from`, ...) are omitted when default.

## Tests

`cargo test -p wartide-engine` runs unit tests per rule family plus a random-playout fuzz test
(`WARTIDE_FUZZ_SEEDS`, `WARTIDE_FUZZ_ROUNDS`, `WARTIDE_FUZZ_STATS` tune it; use `--release` for long runs).
