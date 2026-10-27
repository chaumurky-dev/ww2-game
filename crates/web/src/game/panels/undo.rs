//! Taking back movement: grouping of this phase's moves plus the panel section and the
//! "Undo all moves" confirmation.
use crate::components::modal::Modal;
use crate::game::conv::*;
use crate::game::ctx::GameCtx;
use leptos::prelude::*;
use wartide_engine::action::UndoScope;
use wartide_engine::undo::{moved_this_phase, undoable, AA_MESSAGE};
use wartide_engine::{Action, GameState, LocationId, UnitId, UnitType};

/// Units of one kind that arrived at a destination from the same place.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Where the units stood when the phase began.
    pub from: LocationId,
    pub kind: UnitType,
    /// Cargo that rode a ship (listed under the ship's destination, flagged "aboard").
    pub aboard: bool,
    pub units: Vec<UnitId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DestGroup {
    pub to: LocationId,
    pub rows: Vec<Row>,
}

/// This phase's moves of the acting power, by destination. A unit that moved twice appears once,
/// with `from` the place it started the phase. Cargo still aboard stays listed (flagged), so
/// that "Undo all into <sea zone>" matches what the list shows.
pub fn undo_groups(st: &GameState) -> Vec<DestGroup> {
    let mut out: Vec<DestGroup> = vec![];
    for (id, from, to) in moved_this_phase(st) {
        let Some(u) = st.unit(id).filter(|u| u.owner == st.current_power) else {
            continue;
        };
        let aboard = u.loaded_on.is_some();
        let gi = match out.iter().position(|g| g.to == to) {
            Some(i) => i,
            None => {
                out.push(DestGroup { to, rows: vec![] });
                out.len() - 1
            }
        };
        let rows = &mut out[gi].rows;
        match rows
            .iter_mut()
            .find(|r| r.from == from && r.kind == u.kind && r.aboard == aboard)
        {
            Some(r) => r.units.push(id),
            None => rows.push(Row {
                from,
                kind: u.kind,
                aboard,
                units: vec![id],
            }),
        }
    }
    out.sort_by_key(|g| g.to.0);
    out
}

/// Did any move of this phase draw anti-aircraft fire (which blocks every undo)?
pub fn aa_blocked(st: &GameState) -> bool {
    st.turn.move_journal.iter().any(|e| e.aa_fired)
}

/// Tooltip for a disabled undo button.
fn why_not(st: &GameState) -> String {
    if aa_blocked(st) {
        AA_MESSAGE.to_string()
    } else {
        "These moves cannot be taken back".to_string()
    }
}

fn describe_row(r: &Row) -> String {
    let n = r.units.len() as u32;
    format!(
        "{n} {}{}",
        unit_name(r.kind, n),
        if r.aboard { " (aboard)" } else { "" }
    )
}

/// A button sending `UndoMoves` for `scope`, disabled with an explanation when the engine says no.
pub fn undo_button(
    ctx: GameCtx,
    scope: UndoScope,
    label: impl IntoView + 'static,
    class: &'static str,
) -> impl IntoView {
    let s2 = scope.clone();
    let ok = Memo::new(move |_| ctx.state.with(|st| undoable(st, &scope)));
    let title = move || {
        if ok.get() {
            "Take this move back".to_string()
        } else {
            ctx.state.with(why_not)
        }
    };
    view! {
        <button class=class disabled=move || !ok.get() title=title
            on:click=move |_| ctx.send(Action::UndoMoves { scope: s2.clone() })>
            {label}
        </button>
    }
}

/// "Moved this phase" list with per-row and per-destination undo.
#[component]
pub fn MovedSection() -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    move || {
        let (groups, aa) = ctx.state.with(|st| (undo_groups(st), aa_blocked(st)));
        if groups.is_empty() {
            return ().into_any();
        }
        let items = groups
            .into_iter()
            .map(|g| {
                let to = g.to;
                let rows = g
                    .rows
                    .into_iter()
                    .map(|r| {
                        let text = format!("\u{2190} {}: {}", loc_name(r.from), describe_row(&r));
                        view! {
                            <div class="urow">
                                <span class="grow">{text}</span>
                                {undo_button(ctx, UndoScope::Units { units: r.units }, "Undo", "secondary small")}
                            </div>
                        }
                    })
                    .collect_view();
                view! {
                    <div class="moved-dest">
                        <div class="row">
                            <strong class="grow">{loc_name(to)}</strong>
                            {undo_button(ctx, UndoScope::Into { location: to }, format!("Undo all into {}", loc_name(to)), "secondary small")}
                        </div>
                        {rows}
                    </div>
                }
            })
            .collect_view();
        view! {
            <div class="sub moved-section">
                <strong>"Moved this phase"</strong>
                {aa.then(|| view! { <div class="warn">{AA_MESSAGE}</div> })}
                {items}
            </div>
        }
        .into_any()
    }
}

/// "Undo all moves" button with its confirmation dialog.
#[component]
pub fn UndoAll() -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let open = RwSignal::new(false);
    let count = move || ctx.state.with(|st| moved_this_phase(st).len());
    let ok = move || ctx.state.with(|st| undoable(st, &UndoScope::All));
    let confirm = move || {
        open.set(false);
        ctx.send(Action::UndoMoves {
            scope: UndoScope::All,
        });
    };
    view! {
        <Show when=move || { count() > 0 }>
            <button class="secondary" disabled=move || !ok()
                title=move || if ok() { "Take back every move made this phase".to_string() } else { ctx.state.with(why_not) }
                on:click=move |_| open.set(true)>"Undo all moves"</button>
        </Show>
        <Modal open title="Undo all moves?">
            <p>{move || {
                let n = count();
                format!("Take back every move made this phase? {n} unit{} return{} to where {} started.",
                    if n == 1 { "" } else { "s" }, if n == 1 { "s" } else { "" }, if n == 1 { "it" } else { "they" })
            }}</p>
            <div class="row">
                <button class="danger" data-autofocus on:click=move |_| confirm()>"Undo all"</button>
                <button class="secondary" on:click=move |_| open.set(false)>"Keep moves"</button>
            </div>
        </Modal>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_engine::{apply, GameSettings, Phase, Power};

    fn combat_move() -> GameState {
        let mut st = GameState::new(GameSettings::default());
        apply(&mut st, Power::Ussr, &Action::EndPhase, 1).unwrap();
        apply(&mut st, Power::Ussr, &Action::EndPhase, 2).unwrap();
        assert_eq!(st.phase, Phase::CombatMove);
        st
    }

    fn step(st: &GameState, id: UnitId, not: Option<LocationId>) -> Option<Vec<LocationId>> {
        wartide_engine::query::reachable(st, id)
            .into_iter()
            .find(|r| !r.battle && r.path.len() == 1 && Some(r.to) != not)
            .map(|r| r.path)
    }

    #[test]
    fn empty_before_any_move() {
        assert!(undo_groups(&combat_move()).is_empty());
    }

    #[test]
    fn groups_by_destination_and_origin() {
        let mut st = combat_move();
        let ids: Vec<UnitId> = st
            .units
            .iter()
            .filter(|u| u.owner == Power::Ussr)
            .map(|u| u.id)
            .collect();
        let mut moved = 0;
        for id in ids {
            if let Some(path) = step(&st, id, None) {
                let a = Action::Move {
                    units: vec![id],
                    path,
                };
                if apply(&mut st, Power::Ussr, &a, 3).is_ok() {
                    moved += 1;
                }
            }
        }
        assert!(moved >= 2);
        let g = undo_groups(&st);
        let total: usize = g.iter().flat_map(|g| &g.rows).map(|r| r.units.len()).sum();
        assert_eq!(total, moved);
        for d in &g {
            for r in &d.rows {
                assert_ne!(r.from, d.to);
                assert!(!r.aboard);
            }
        }
        assert!(g.windows(2).all(|w| w[0].to.0 < w[1].to.0));
    }

    #[test]
    fn unit_moved_twice_keeps_original_origin() {
        let mut st = combat_move();
        let (id, from) = st
            .units
            .iter()
            .filter(|u| u.owner == Power::Ussr && u.kind == UnitType::Armor)
            .find(|u| step(&st, u.id, None).is_some())
            .map(|u| (u.id, u.location))
            .expect("a Soviet armor that can step");
        let first = step(&st, id, None).unwrap();
        apply(
            &mut st,
            Power::Ussr,
            &Action::Move {
                units: vec![id],
                path: first,
            },
            1,
        )
        .unwrap();
        let second = step(&st, id, Some(from)).expect("armor has a second step");
        let to = second[0];
        apply(
            &mut st,
            Power::Ussr,
            &Action::Move {
                units: vec![id],
                path: second,
            },
            2,
        )
        .unwrap();
        let g = undo_groups(&st);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].to, to);
        assert_eq!(g[0].rows.len(), 1);
        assert_eq!(g[0].rows[0].from, from);
        assert_eq!(g[0].rows[0].units, vec![id]);
    }

    #[test]
    fn cargo_is_listed_aboard_under_the_ships_destination() {
        let mut st = combat_move();
        let ids: Vec<UnitId> = st
            .units
            .iter()
            .filter(|u| u.owner == Power::Ussr && u.kind == UnitType::Infantry)
            .map(|u| u.id)
            .take(2)
            .collect();
        let from = st.unit(ids[0]).unwrap().location;
        let sea = st
            .units
            .iter()
            .find(|u| u.location != from)
            .map(|u| u.location)
            .unwrap();
        // Pretend the infantry rode a ship to `sea` (grouping only compares snapshot and now).
        for u in st.units.iter_mut().filter(|u| ids.contains(&u.id)) {
            u.location = sea;
            u.loaded_on = Some(9999);
        }
        let g = undo_groups(&st);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].to, sea);
        assert_eq!(g[0].rows.len(), 1);
        assert!(g[0].rows[0].aboard);
        assert_eq!(describe_row(&g[0].rows[0]), "2 infantry (aboard)");
    }
}
