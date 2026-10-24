//! Drives a random-policy game and exercises the pure UI helpers at every step (no panics, and
//! the suggested casualty picks satisfy the picker's own validity rule).
use super::dice::dice_shows;
use super::log::format_events;
use super::model::{board_model, Overlay};
use super::moves::{group_reach, movable_locations, stranded_air, unit_groups};
use wartide_engine::casualties::{candidates, default_pick, required_casualties};
use wartide_engine::random_policy::RandomPolicy;
use wartide_engine::{apply, pending_decision, Decision, GameSettings, GameState, Phase};

#[test]
fn ui_helpers_survive_random_games() {
    for game_seed in 0..3u64 {
        let mut st = GameState::new(GameSettings::default());
        let mut pol = RandomPolicy::new(game_seed + 100);
        for n in 0..1200u64 {
            let before = st.clone();
            let Some((p, a)) = pol.next_action(&st) else {
                break;
            };
            let Ok(ev) = apply(&mut st, p, &a, game_seed * 10_000 + n) else {
                continue;
            };
            let _ = format_events(&ev, before.round);
            let _ = dice_shows(&before, &st, &ev, 0);
            let _ = board_model(&st, &Overlay::default());
            match pending_decision(&st) {
                Decision::Phase {
                    power,
                    phase: Phase::CombatMove | Phase::NonCombatMove,
                } if n % 7 == 0 => {
                    let strict = st.phase == Phase::NonCombatMove;
                    let _ = stranded_air(&st, power, strict);
                    for loc in movable_locations(&st, power) {
                        for g in unit_groups(&st, loc, power) {
                            if g.can_move {
                                let r = group_reach(&st, &g.ids[..1]);
                                assert!(!r.is_empty(), "can_move group without destinations");
                            }
                        }
                    }
                }
                Decision::SelectCasualties { hits, sub_hits, .. } => {
                    let b = st.battle.as_ref().unwrap();
                    let side = b.pending.as_ref().unwrap().side;
                    let cand = candidates(&st, b, side);
                    let normal = hits - sub_hits;
                    let (need, min_nonair) = required_casualties(&st, &cand, normal, sub_hits);
                    let pick = default_pick(&st, &cand, normal, sub_hits);
                    let nonair = pick
                        .iter()
                        .filter(|i| st.unit(**i).is_some_and(|u| !u.kind.is_air()))
                        .count();
                    assert_eq!(pick.len(), need);
                    assert!(nonair >= min_nonair);
                }
                _ => {}
            }
        }
    }
}
