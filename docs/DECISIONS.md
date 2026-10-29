# Decisions log

- 2026-10-24: Project name "Wartide". Rust workspace; axum server, Leptos CSR web client,
  Postgres via sqlx, AWS ECS Fargate deploy via GitHub Actions + Terraform.
- 2026-10-24: Rules edition: Classic, 2nd edition (1986). Optional rules (economic victory,
  weapons development) are exposed as per-game settings with Classic defaults.
- 2026-10-24: Ratings use Glicko-2, maintained separately per ladder (public 2p, public 5p,
  and per-group). Private games never affect the public ladders.
- 2026-10-24: Private games always have five power seats; the host assigns each seat to a human
  (one human may hold several seats) or a bot persona. 2-player public = one human per team;
  5-player public = one human per power.
- 2026-10-24: Server stores an append-only action log plus the latest state snapshot; every action's
  dice come from `seed = hash(game_seed, seq)` so games replay deterministically.
- 2026-10-24 (engine): When defenders of several allied powers share a battle, hits are assigned by a
  single `SelectCasualties` from the "defending controller": the territory's controlling power if it has
  units in the battle, otherwise the defending power with the most units there (ties: turn order). Each
  defending power still rolls its own dice (one `DiceRolled` event per power). Forced choices (all units
  are lost, only submarine hits against a lone non-air target, or all eligible units identical) are
  auto-assigned without a prompt.
- 2026-10-24 (engine): AA guns fire at the moment an enemy aircraft enters their territory during combat
  movement (including the destination), one die per aircraft per entry; this replaces the "start of
  battle" shot of RULES 5.2 and is statistically identical. The attacker's choice of which aircraft die
  is automated: cheapest aircraft first. Guns carried on transports never fire. Aircraft that merely end
  combat movement in an AA territory with no battle and no raid are shot at on entry only.
- 2026-10-24 (engine): Unloading from a transport into an enemy-occupied territory (or from a hostile
  sea zone) during combat movement is *deferred*: the cargo stays aboard flagged with its target and
  lands at the start of that territory's battle, after any sea battle in the zone is resolved; if the
  zone is not cleared (or the transports retreat) the assault is cancelled and the cargo stays aboard.
  Unloading into empty enemy/neutral territory from a quiet zone captures immediately.
- 2026-10-24 (engine): The Combat phase ends automatically as soon as no battle, bombing raid or rocket
  launch remains possible; otherwise the player presses `EndPhase` (only allowed once no battle is
  pending). Amphibious land battles run to the end without a retreat prompt. Defending submarines are
  only asked about withdrawal when a legal destination exists.
- 2026-10-24 (engine): Research dice are bought once per turn (`BuyResearch{dice}`; 0 dice is allowed and
  uses up the option). `Concede` records the power; when every power of a team has conceded the other team
  wins. Unconceded powers keep playing (the server hands the seat to a bot). A power with no units and no
  territories still runs its turn (income 0) automatically so turn order stays stable.
- 2026-10-24 (engine): Placing a new industrial complex does not consume any allowance of the complex
  it is placed on; units produced from a complex placed this turn must wait for the next turn. Initial
  printed pieces are treated as pre-existing (complexes are usable on turn 1). Allies' transports are
  assumed to consent to carrying your units. Not implemented: the optional "individual winner" scoring
  and "naval placement in enemy waters" variants of RULES 1.1.
- 2026-10-24 (engine): "No kamikaze" is enforced when a plane ends a combat-movement leg: some friendly
  landing place (land held at turn start, or a friendly carrier with room where it stands now) must be
  reachable with the remaining range, ignoring neutrals. Aircraft still airborne when non-combat movement
  ends land on a carrier in their zone if possible, otherwise are lost.
- 2026-10-24 (engine audit): A defending fighter whose carrier is sunk re-lands on another friendly carrier
  with room in the zone, otherwise on a friendly *island* territory adjacent to that zone (RULES 11 says
  "carrier slot or friendly island"; on our map islands are separate territories touching the zone, so
  "in the zone" is read as "adjacent island that is friendly and not enemy-occupied"); otherwise it is lost.
  Mainland coasts do not count. Research and purchase quantities use saturating arithmetic so absurd counts
  are rejected as insufficient funds instead of overflowing.
- 2026-10-25 (product): "Players" when creating a private game means *human* players, never seats. One
  human plays a whole side (Axis or Allies, their choice) against the computer, and each enemy power
  gets its own bot seat and persona so the other side has several voices; nobody is offered "all five
  powers" because that is not a game. Two to five humans get the classic splits (Axis vs Allies;
  Axis / USSR / UK+USA; Germany / Japan / USSR / UK+USA; one each) with the non-host seats open for
  friends. Any other arrangement (e.g. two friends as the Allies against bot Axis, or one human as the
  USSR with bot allies) is reached through the "Custom" split, which frees the seat count and the
  power split. `preset_layout(1)` therefore equals `preset_layout(2)`, and `solo_seats(side)` builds
  the one-human layout.
- 2026-10-25 (product): While one player takes their turn, nobody else has anything to do. Whenever the
  engine asks a *defending* power to choose casualties or to withdraw submarines, the server answers for
  a human seat at once (`Actor::Auto`, driven by `bots::run_pass`) from two per-user settings, and the
  defender gets no deadline and no "your turn" mail for it. Bot seats and timeout takeovers keep their
  own code path. `casualty_policy` (default `cheapest_first`) orders who dies, ties broken by unit id:
  `cheapest_first` = lowest IPC cost, then lowest defence; `keep_best_defenders` = lowest defence, then
  lowest cost (bombers and transports before infantry, fighters last); `protect_aircraft` = cheapest
  first with every aircraft after every non-aircraft; `keep_counterattack` = infantry first, then other
  units by cost, armor and aircraft last. Submarine hits are always assigned to non-aircraft first, as
  the rules require. `sub_withdrawal` (default `when_outgunned`): `never` stays, `always` withdraws to
  the first legal zone, `when_outgunned` uses the AI's odds heuristic. The ruleset has no two-hit
  battleship, so there is no free "first hit" to prefer. Attackers still choose their own casualties.
- 2026-10-26 (engine): The engine's own default casualty pick (used when a battle is auto-resolved) is
  now `choose_casualties` under `CasualtyPolicy::default()` rather than a separate copy of the
  algorithm. The only observable difference is the tie-break among equal-cost units: previously by unit
  id, now by defence value then id (so an AA gun dies before armor, a transport before a submarine).
  Replays of games recorded before this date that hit such a tie may diverge; the server stores the
  full state after every action, so only client-side history stepping is affected.
- 2026-10-27 (map audit): Audited `crates/engine/data/map.json` against the community "World War II Classic" setup file
  (triplea-maps/world_war_ii_classic, `map/games/classic.xml`, the file the TripleA forum lists as the map matching
  the 1986 classic board) with the reusable `scripts/audit_map.py`: 70 land territories, 58 sea zones, every
  IPC value, original owner, capital, complex, neutral flag, land/sea/sea adjacency, the Suez and Panama canals
  and all starting units match. The only difference is the deliberate Gulf of Mexico - Mexico sea-zone edge
  (RULES 14). Name variants are intentional (rulebook names, RULES 18; spelling fixes such as
  Mediterranean/Caribbean). Ukraine S.S.R. borders Eastern Europe, Karelia S.S.R., Caucasus and the Black
  Sea only; it does **not** border Russia or Southern Europe on the classic board (the file's polygons agree:
  Ukraine spans x 1264-1414 and Russia starts at x 1476 on its 3500-wide map, with Caucasus between), so no
  adjacency or region change was made. Not independently verified against a scan of the printed board: the
  forum threads and fan wikis that discuss it were unreachable (Cloudflare 403/504) from the audit environment.
- 2026-10-27 (engine): `Action::UndoMoves { scope }` takes back movement within the open Combat Move or
  Non-Combat Move phase. The engine snapshots units, `next_unit_id`, control and treasuries when such a
  phase begins (`TurnState.phase_start`) and journals each successful `Move`/`Load`/`Unload`
  (`TurnState.move_journal`, with an `aa_fired` flag). Undo restores the snapshot and replays the journal
  entries that were not selected, through the normal validation, with a throwaway RNG and event list; an
  entry that no longer validates (or would roll dice) is dropped too and reported as `cascaded`. Both
  fields are cleared when the phase is left, so the stored state carries the snapshot only during movement
  phases, and old states without them still deserialize (undo then reports nothing to take back).
  Antiaircraft fire is the only dice in movement. To rule out re-rolling it under a different seed, any
  journal entry that rolled dice (even a roll with no hit) makes every undo in that phase illegal
  ("units that were fired on by anti-aircraft guns cannot take back their move"); this is stricter than
  refusing only the affected entry because the fired entry could not be re-applied faithfully. Scope
  `into` matches `Move` by last path step, `Unload` by `to`, and `Load` by the transport's current sea
  zone. Bots and the random policy never emit the action.
- 2026-10-29 (infra): RDS `prevent_destroy` replaced by `deletion_protection` + a hibernate switch.
  `lifecycle.prevent_destroy` must be a literal, so it would make any plan with `hibernate = true` (which sets
  the instance's `count` to 0) fail, and removing it by hand for each teardown was the only way to destroy. The
  data-loss guard is now AWS-side `deletion_protection` (`db_deletion_protection`, default true), which is
  enforced by RDS itself and also covers console and CLI deletes, plus the final snapshot
  (`wartide-prod-final-<tag>`). `scripts/aws-hibernate.sh down` turns protection off in a separate apply first
  (destroy uses the recorded state values), so an accidental `terraform destroy` still fails unless protection was
  deliberately turned off. `ignore_changes = [snapshot_identifier]` keeps later applies from replacing a database
  that was restored from a snapshot. ALB deletion protection became `alb_deletion_protection` for the same reason.

