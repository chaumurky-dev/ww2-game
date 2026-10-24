//! Dice display: per-die hit numbers reconstructed from the battle state, and the `Die` view.
use super::conv::*;
use crate::model::Power as UiPower;
use crate::model::PowerExt;
use leptos::prelude::*;
use wartide_engine::event::DiceContext;
use wartide_engine::state::BattleKind;
use wartide_engine::tech::{attack_dice, attack_value, defense_value};
use wartide_engine::{Event, GameState, LocationId, Power, Unit, UnitId, UnitType};

/// One die: its face and whether it hit (None when hits are not defined for this roll).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DieFace {
    pub value: u8,
    pub hit: Option<bool>,
}

/// A group of dice rolled together (one `DiceRolled` event).
#[derive(Clone, Debug, PartialEq)]
pub struct DiceShow {
    /// Unique per ingest so the roll-in animation replays.
    pub id: u64,
    pub label: String,
    pub power: UiPower,
    pub location: Option<LocationId>,
    pub is_attack: bool,
    pub dice: Vec<DieFace>,
}

fn ctx_location(c: &DiceContext) -> Option<LocationId> {
    match c {
        DiceContext::Research | DiceContext::ResearchTech => None,
        DiceContext::AaFire { location }
        | DiceContext::Bombardment { location }
        | DiceContext::FirstStrike { location, .. }
        | DiceContext::Attack { location, .. }
        | DiceContext::Defense { location, .. }
        | DiceContext::StrategicBombing { location }
        | DiceContext::Rocket { location } => Some(*location),
    }
}

/// One possible reading of who is in the battle when dice were rolled.
struct Roster {
    attackers: Vec<UnitId>,
    defenders: Vec<UnitId>,
    sea: bool,
}

/// Hit numbers of the dice in roll order for a roster (None if undeterminable).
fn thresholds(
    tech: &GameState,
    unit: &dyn Fn(UnitId) -> Option<Unit>,
    r: &Roster,
    c: &DiceContext,
    power: Power,
) -> Option<Vec<u8>> {
    match c {
        DiceContext::Attack { .. } => {
            let mut v = vec![];
            for &id in &r.attackers {
                let u = unit(id)?;
                if u.kind == UnitType::Submarine && r.sea {
                    continue;
                }
                v.extend((0..attack_dice(tech, &u)).map(|_| attack_value(tech, &u)));
            }
            Some(v)
        }
        DiceContext::Defense { .. } => {
            let mut v = vec![];
            for &id in &r.defenders {
                let u = unit(id)?;
                if u.owner == power && defense_value(tech, &u) > 0 {
                    v.push(defense_value(tech, &u));
                }
            }
            Some(v)
        }
        DiceContext::FirstStrike { .. } => Some(
            r.attackers
                .iter()
                .filter_map(|&i| unit(i))
                .filter(|u| u.kind == UnitType::Submarine)
                .map(|u| attack_value(tech, &u))
                .collect(),
        ),
        _ => None,
    }
}

/// Rosters the battle at `loc` may have had when event `i` of the batch happened.
fn rosters(
    before: &GameState,
    after: &GameState,
    events: &[Event],
    i: usize,
    loc: LocationId,
) -> Vec<Roster> {
    let mut out = vec![];
    let removed: Vec<UnitId> = events[..i]
        .iter()
        .flat_map(|e| match e {
            Event::CasualtiesRemoved { units, .. } | Event::UnitsLost { units, .. } => {
                units.iter().map(|u| u.id).collect()
            }
            _ => vec![],
        })
        .collect();
    let keep = |v: &[UnitId]| {
        v.iter()
            .copied()
            .filter(|x| !removed.contains(x))
            .collect::<Vec<_>>()
    };
    if let Some(Event::BattleStarted {
        attackers,
        defenders,
        kind,
        ..
    }) = events[..i]
        .iter()
        .rev()
        .find(|e| matches!(e, Event::BattleStarted { location, .. } if *location == loc))
    {
        out.push(Roster {
            attackers: keep(&attackers.iter().map(|u| u.id).collect::<Vec<_>>()),
            defenders: keep(&defenders.iter().map(|u| u.id).collect::<Vec<_>>()),
            sea: *kind == BattleKind::Sea,
        });
    }
    for st in [before, after] {
        if let Some(b) = st.battle.as_ref().filter(|b| b.location == loc) {
            out.push(Roster {
                attackers: keep(&b.attackers),
                defenders: keep(&b.defenders),
                sea: b.kind == BattleKind::Sea,
            });
        }
    }
    out
}

/// Build the dice groups of a batch of events. `before`/`after` are the states around the batch;
/// the battle rosters (from `BattleStarted` or the states) supply the per-die hit numbers.
pub fn dice_shows(
    before: &GameState,
    after: &GameState,
    events: &[Event],
    id_base: u64,
) -> Vec<DiceShow> {
    let unit = |id: UnitId| before.unit(id).or_else(|| after.unit(id)).cloned();
    let mut out = vec![];
    for (i, e) in events.iter().enumerate() {
        let Event::DiceRolled {
            power,
            context,
            rolls,
            ..
        } = e
        else {
            continue;
        };
        let th: Option<Vec<u8>> = match context {
            DiceContext::Bombardment { .. } => Some(vec![4; rolls.len()]),
            DiceContext::AaFire { .. } => Some(vec![1; rolls.len()]),
            _ => ctx_location(context).and_then(|loc| {
                rosters(before, after, events, i, loc)
                    .iter()
                    .filter_map(|r| thresholds(before, &unit, r, context, *power))
                    .find(|t| t.len() == rolls.len())
            }),
        };
        let dice = rolls
            .iter()
            .enumerate()
            .map(|(k, &v)| DieFace {
                value: v,
                hit: match context {
                    DiceContext::Research => Some(v == 6),
                    DiceContext::ResearchTech => None,
                    _ => th.as_ref().and_then(|t| t.get(k)).map(|&n| v <= n),
                },
            })
            .collect();
        let label = match context {
            DiceContext::Attack { round, .. } => format!("Attack (round {round})"),
            DiceContext::Defense { round, .. } => format!("Defense (round {round})"),
            DiceContext::FirstStrike { .. } => "Submarine strike".into(),
            DiceContext::Bombardment { .. } => "Bombardment".into(),
            DiceContext::AaFire { .. } => "AA fire".into(),
            DiceContext::Research => "Research (6 = breakthrough)".into(),
            DiceContext::ResearchTech => "Technology".into(),
            DiceContext::StrategicBombing { .. } => "Bombing raid damage".into(),
            DiceContext::Rocket { .. } => "Rocket damage".into(),
        };
        out.push(DiceShow {
            id: id_base + i as u64,
            label,
            power: ui_power(*power),
            location: ctx_location(context),
            is_attack: matches!(
                context,
                DiceContext::Attack { .. }
                    | DiceContext::FirstStrike { .. }
                    | DiceContext::Bombardment { .. }
            ),
            dice,
        });
    }
    out
}

/// Which of the 9 grid cells hold a pip for each face.
fn pips(v: u8) -> [bool; 9] {
    let on: &[usize] = match v {
        1 => &[4],
        2 => &[0, 8],
        3 => &[0, 4, 8],
        4 => &[0, 2, 6, 8],
        5 => &[0, 2, 4, 6, 8],
        _ => &[0, 2, 3, 5, 6, 8],
    };
    let mut a = [false; 9];
    for &i in on {
        a[i] = true;
    }
    a
}

/// A die face: cream pips on a dark die, rolls in within 600 ms (staggered).
#[component]
pub fn Die(
    face: DieFace,
    #[prop(default = 0)] index: usize,
    #[prop(default = true)] animate: bool,
) -> impl IntoView {
    let delay = (index * 25).min(100);
    let class = match (face.hit, animate) {
        (Some(true), true) => "dieface hit roll",
        (Some(false), true) => "dieface miss roll",
        (None, true) => "dieface roll",
        (Some(true), false) => "dieface hit",
        (Some(false), false) => "dieface miss",
        (None, false) => "dieface",
    };
    view! {
        <span class=class style=format!("animation-delay:{delay}ms") title=face.value.to_string()>
            {pips(face.value).into_iter().map(|p| view! { <i class:pip=p></i> }).collect_view()}
        </span>
    }
}

/// Dice rolled by the last action for a location (`None` = research / location-less dice).
#[component]
pub fn DiceBoard(#[prop(optional)] at: Option<LocationId>) -> impl IntoView {
    let ctx = expect_context::<super::ctx::GameCtx>();
    view! {
        {move || {
            let me = ctx.state.with(|s| s.current_power);
            ctx.dice
                .get()
                .into_iter()
                .filter(|d| d.location == at && (at.is_some() || d.power == ui_power(me)))
                .map(|d| view! { <DiceRow show=d /> })
                .collect_view()
        }}
    }
}

/// A labelled row of dice with the hit count when known.
#[component]
pub fn DiceRow(show: DiceShow) -> impl IntoView {
    let ctx = expect_context::<super::ctx::GameCtx>();
    let fresh = ctx.first_view(show.id);
    let hits = show.dice.iter().filter(|d| d.hit == Some(true)).count();
    let known = show.dice.iter().any(|d| d.hit.is_some());
    view! {
        <div class="dicerow">
            <div class="muted">{format!("{}: ", show.power.id().to_uppercase())}{show.label.clone()}{known.then(|| format!(" - {hits} hit{}", if hits == 1 { "" } else { "s" }))}</div>
            <div class="dice">
                {show.dice.iter().enumerate().map(|(i, d)| view! { <Die face=*d index=i animate=fresh /> }).collect_view()}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_engine::{apply, Action, GameSettings, MapData};

    #[test]
    fn pips_match_faces() {
        for v in 1..=6u8 {
            assert_eq!(pips(v).iter().filter(|p| **p).count(), v as usize);
        }
    }

    #[test]
    fn research_dice_hit_on_six() {
        let st = GameState::new(GameSettings::default());
        let ev = vec![Event::DiceRolled {
            power: Power::Ussr,
            context: DiceContext::Research,
            rolls: vec![6, 2],
            details: vec![],
        }];
        let d = dice_shows(&st, &st, &ev, 10);
        assert_eq!(d[0].dice[0].hit, Some(true));
        assert_eq!(d[0].dice[1].hit, Some(false));
        assert_eq!(d[0].id, 10);
    }

    #[test]
    fn battle_dice_get_thresholds() {
        // Drive a real battle from the printed setup with the random policy until dice appear.
        let mut st = GameState::new(GameSettings::default());
        let mut pol = wartide_engine::random_policy::RandomPolicy::new(7);
        let mut seen = 0;
        for seed in 0..4000u64 {
            let before = st.clone();
            let Some((p, a)) = pol.next_action(&st) else {
                break;
            };
            let Ok(ev) = apply(&mut st, p, &a, seed) else {
                continue;
            };
            for d in dice_shows(&before, &st, &ev, 0) {
                if d.label.starts_with("Attack") || d.label.starts_with("Defense") {
                    seen += 1;
                    assert!(
                        d.dice.iter().all(|x| x.hit.is_some()),
                        "thresholds missing for {:?} after {:?}",
                        d,
                        a
                    );
                }
            }
            if seen > 20 {
                break;
            }
        }
        assert!(seen > 0, "no battle dice seen");
        let _ = (MapData::get(), Action::EndPhase);
    }
}
