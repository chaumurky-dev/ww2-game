//! `/help`: a short, original-wording summary of the game. The unit table is generated from the
//! engine's unit data so it can never drift from the rules.
use crate::components::unit_icon::UnitIcon;
use crate::game::conv::{ui_kind, ui_power};
use crate::model::PowerExt;
use leptos::prelude::*;
use wartide_engine::{Power, UnitType};

fn unit_note(k: UnitType) -> &'static str {
    match k {
        UnitType::Infantry => "Cheap, holds ground. Can be carried by transports.",
        UnitType::Armor => "Fast land striker; may blitz through empty enemy land.",
        UnitType::Fighter => {
            "Flies out and must land at the end of the turn; can sit on a carrier."
        }
        UnitType::Bomber => "Long-range flyer; can raid enemy industry instead of fighting.",
        UnitType::Battleship => "Tough warship; can shell a coast during an amphibious landing.",
        UnitType::Carrier => "Holds two friendly fighters at sea.",
        UnitType::Transport => "Ferries land units; weak in a fight.",
        UnitType::Submarine => "Cheap raider; can slip away before a battle.",
        UnitType::AaGun => "Shoots at aircraft that attack its territory.",
        UnitType::IndustrialComplex => "Lets a territory build units; a stand-alone structure.",
    }
}

#[component]
pub fn Help() -> impl IntoView {
    let rows = UnitType::ALL
        .into_iter()
        .map(|k| {
            let s = k.stats();
            view! {
                <tr>
                    <td><UnitIcon kind=ui_kind(k) power=ui_power(Power::Ussr) /></td>
                    <td>{ui_kind(k).name()}</td>
                    <td class="num">{s.cost}</td>
                    <td class="num">{s.attack}</td>
                    <td class="num">{s.defense}</td>
                    <td class="num">{s.movement}</td>
                    <td class="muted">{unit_note(k)}</td>
                </tr>
            }
        })
        .collect_view();
    let powers = Power::ALL
        .into_iter()
        .map(|p| view! { <li><strong>{ui_power(p).name()}</strong>{format!(" ({:?})", p.team())}</li> })
        .collect_view();
    view! {
        <div class="page col help">
            <div class="card col">
                <h2>"How Wartide works"</h2>
                <p class="serif">"Five powers fight a world war over a shared map. Every power earns IPC (the money of the game) from the territories it holds, buys units with it, moves them, and settles disputes by rolling dice. The Axis (Germany, Japan) race the Allies (Soviet Union, United Kingdom, United States). Games are play-by-web: you take your turn whenever you like before the deadline, and the computer players move as fast as they can."</p>
                <ul class="plain">{powers}</ul>
                <p class="muted">"Turn order is Soviet Union, Germany, United Kingdom, Japan, United States; one lap is a round. The complete rules live in docs/RULES.md."</p>
            </div>
            <div class="card col">
                <h2>"Turn phases"</h2>
                <ol class="phases">
                    <li><strong>Research</strong>" (if enabled): pay 5 IPC per die; sixes can unlock a technology."</li>
                    <li><strong>Purchase</strong>": spend your IPC. New units wait until the end of the turn."</li>
                    <li><strong>Combat move</strong>": send units into enemy territory or sea zones. Fights are not resolved yet."</li>
                    <li><strong>Combat</strong>": resolve every battle you started, in the order you choose."</li>
                    <li><strong>Non-combat move</strong>": reposition units that did not move; aircraft must find somewhere to land."</li>
                    <li><strong>Place</strong>": put the units you bought on the board next to or on your industrial complexes."</li>
                    <li><strong>Income</strong>": collect IPC equal to the value of what you hold (not while your capital is occupied)."</li>
                </ol>
                <p class="muted">"On the board: click a stack that glows, pick units in the panel, click a highlighted destination. Esc clears your selection; Enter presses the main button of the panel."</p>
            </div>
            <div class="card col">
                <h2>"Units"</h2>
                <div style="overflow-x:auto"><table>
                    <thead><tr><th></th><th>"Unit"</th><th class="num">"Cost"</th><th class="num">"Attack"</th><th class="num">"Defence"</th><th class="num">"Move"</th><th>"Notes"</th></tr></thead>
                    <tbody>{rows}</tbody>
                </table></div>
                <p class="muted">"Attack and defence are the highest die that scores a hit (one die per unit; for example 3 hits on a 1, 2 or 3). Figures come straight from the rules engine."</p>
            </div>
            <div class="card col">
                <h2>"Combat basics"</h2>
                <ul>
                    <li>"Both sides roll one die per unit at the same time. Rolls at or under the unit's number are hits."</li>
                    <li>"Each hit removes one unit of the opponent's choice. You pick your own casualties; the panel pre-selects a sensible default. Defenders get asked too, even on a bot's turn."</li>
                    <li>"Casualties are removed after both sides have rolled, so a unit that dies still fires in that round (submarine first strikes and anti-aircraft fire are the exceptions)."</li>
                    <li>"After every round the attacker may press on or retreat. Amphibious landings cannot retreat."</li>
                    <li>"When the last defender is gone and a land unit survives, the attacker takes the territory and its income."</li>
                    <li>"At sea, attacking submarines get a first strike that removes a ship before it can shoot back, and their hits can never be assigned to aircraft. Submarines may also withdraw between rounds."</li>
                </ul>
            </div>
            <div class="card col">
                <h2>"Winning"</h2>
                <ul>
                    <li><strong>Allies</strong>": hold both Germany and Japan at the end of a turn."</li>
                    <li><strong>Axis</strong>": hold two of Russia, the United Kingdom and Eastern U.S.A., or reach the economic threshold (84 IPC by default) at the end of a round."</li>
                    <li>"Optional: an Allied economic threshold, total victory (winners must also keep their own capitals), and balance tweaks. Each game lists its options when you create it."</li>
                    <li>"Lose the capital and a power stops collecting income until it is liberated."</li>
                </ul>
            </div>
        </div>
    }
}
