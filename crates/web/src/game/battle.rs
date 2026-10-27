//! Compact side-panel summary of the battle in progress; the full board is the pop-up in
//! `battle_board.rs`.
use super::battle_board::{hits_pending, viewer_sees, BoardOpen};
use super::conv::*;
use super::ctx::GameCtx;
use leptos::prelude::*;
use wartide_engine::state::BattleKind;

#[component]
pub fn BattleBoard() -> impl IntoView {
    let ctx = expect_context::<GameCtx>();
    let open = use_context::<BoardOpen>().map(|o| o.0);
    view! {
        {move || {
            let st = ctx.state.get();
            let Some(b) = st.battle.clone() else { return ().into_any() };
            let sea = b.kind == BattleKind::Sea;
            let (on_att, on_def) = hits_pending(&b);
            let can_open = open.is_some() && viewer_sees(ctx, &st, &b);
            view! {
                <div class="battle-board">
                    <h3>{format!("Battle: {} \u{b7} round {}", loc_name(b.location), b.round)}</h3>
                    <div class="muted">{format!("{}{}", if sea { "Sea battle" } else { "Land battle" }, if b.amphibious { " \u{b7} amphibious assault" } else { "" })}</div>
                    <div class="muted">{format!("{} attackers vs {} defenders \u{b7} hits waiting: {on_att} on attacker, {on_def} on defender", b.attackers.len(), b.defenders.len())}</div>
                    {can_open.then(|| view! {
                        <div class="row"><button class="primary" on:click=move |_| if let Some(o) = open { o.set(true) }>"Open battle board"</button></div>
                    })}
                </div>
            }.into_any()
        }}
    }
}
