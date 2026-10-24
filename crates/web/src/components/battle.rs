use super::unit_icon::UnitBadge;
use crate::mock::{Power, UnitKind};
use leptos::prelude::*;

fn d6() -> u8 {
    (js_sys::Math::random() * 6.0) as u8 % 6 + 1
}

fn dice_view(rolls: Vec<u8>, hit_at: Vec<u8>) -> impl IntoView {
    rolls
        .into_iter()
        .zip(hit_at)
        .map(|(r, need)| {
            view! { <span class=if r <= need { "die hit dice-roll" } else { "die dice-roll" }>{r}</span> }
        })
        .collect_view()
}

/// Battle board with a mock dice roll. TODO(protocol): drive from engine battle events.
#[component]
pub fn BattleBoard(
    place: String,
    attacker: Power,
    defender: Power,
    #[prop(into)] on_resolved: Callback<String>,
) -> impl IntoView {
    // (unit, count, strength) per side
    let att = [
        (UnitKind::Infantry, 4_u32, 1_u8),
        (UnitKind::Armor, 2, 3),
        (UnitKind::Fighter, 1, 3),
    ];
    let def = [(UnitKind::Infantry, 3_u32, 2_u8), (UnitKind::AaGun, 1, 1)];
    let att_dice = RwSignal::new(Vec::<(u8, u8)>::new());
    let def_dice = RwSignal::new(Vec::<(u8, u8)>::new());
    let round = RwSignal::new(0_u32);
    let roll = move |_| {
        let roll_side = |units: &[(UnitKind, u32, u8)]| {
            units
                .iter()
                .flat_map(|(_, n, s)| (0..*n).map(move |_| (d6(), *s)))
                .collect::<Vec<_>>()
        };
        att_dice.set(roll_side(&att));
        def_dice.set(roll_side(&def));
        round.update(|r| *r += 1);
    };
    let hits =
        move |d: RwSignal<Vec<(u8, u8)>>| d.with(|v| v.iter().filter(|(r, s)| r <= s).count());
    let side = |units: &[(UnitKind, u32, u8)], p: Power| {
        units
            .iter()
            .map(|(k, n, _)| view! { <UnitBadge kind=*k power=p count=*n /> })
            .collect_view()
    };
    let (place_a, place_b) = (place.clone(), place.clone());
    view! {
        <div class="col">
            <h3>{format!("Battle: {place}")}</h3>
            <div class="battle">
                <div>
                    <strong>"Attacker"</strong>
                    <div class="row">{side(&att, attacker)}</div>
                    <div>{move || dice_view(att_dice.get().iter().map(|d| d.0).collect(), att_dice.get().iter().map(|d| d.1).collect())}</div>
                    <div class="muted">{move || format!("{} hits", hits(att_dice))}</div>
                </div>
                <div>
                    <strong>"Defender"</strong>
                    <div class="row">{side(&def, defender)}</div>
                    <div>{move || dice_view(def_dice.get().iter().map(|d| d.0).collect(), def_dice.get().iter().map(|d| d.1).collect())}</div>
                    <div class="muted">{move || format!("{} hits", hits(def_dice))}</div>
                </div>
            </div>
            <div class="row">
                <button on:click=roll>{move || if round.get() == 0 { "Roll dice" } else { "Next round" }}</button>
                <button class="secondary" on:click=move |_| on_resolved.run(format!("Pressed the attack at {place_a}"))>"Press attack"</button>
                <button class="secondary" on:click=move |_| on_resolved.run(format!("Retreated from {place_b}"))>"Retreat"</button>
            </div>
        </div>
    }
}
