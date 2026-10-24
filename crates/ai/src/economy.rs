//! Research and purchasing.
use crate::eval::{bfs, Intel};
use crate::Style;
use wartide_engine::economy::{complex_capacity_left, usable_complex};
use wartide_engine::query::purchase_options;
use wartide_engine::state::PurchaseItem;
use wartide_engine::{Action, GameState, LocationId, MapData, Power, UnitType};

/// What the purchase did, for commentary.
#[derive(Clone, Debug, Default)]
pub struct BuyNote {
    /// Items bought.
    pub items: Vec<(UnitType, u32)>,
    /// A new complex is planned at this territory.
    pub complex_at: Option<LocationId>,
    /// Total IPC spent.
    pub spent: u32,
    /// Capital looked threatened, so the buy is defensive.
    pub defensive: bool,
}

/// Research decision (Aggressive style only).
pub fn plan_research(state: &GameState, me: Power, style: Style) -> Option<Action> {
    let t = state.treasury[me];
    let inc = wartide_engine::income(state, me);
    if style == Style::Aggressive
        && state.settings.weapons_development
        && !state.turn.researched
        && t >= 5
        && f64::from(t) > f64::from(inc) * 1.3
    {
        return Some(Action::BuyResearch { dice: 1 });
    }
    None
}

fn count(state: &GameState, me: Power, k: UnitType) -> u32 {
    state
        .units
        .iter()
        .filter(|u| u.owner == me && u.kind == k)
        .count() as u32
}

/// Is `t` a sensible place for a new complex?
fn complex_site(state: &GameState, me: Power, intel: &Intel) -> Option<LocationId> {
    let map = MapData::get();
    let mut best: Option<(f64, LocationId)> = None;
    for t in map.territories() {
        if state.control[t] != Some(me)
            || state.turn.start_control[t] != Some(me)
            || state.complex_at(t).is_some()
            || map.ipc(t) < 3
        {
            continue;
        }
        let g = intel.grad[t.idx()];
        if g >= 900.0 || g > 10.0 {
            continue;
        }
        if intel.threat[t.idx()] > intel.defense[t.idx()] * 1.2 + 4.0 {
            continue;
        }
        let score = f64::from(map.ipc(t)) * 2.0 - g;
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, t));
        }
    }
    best.map(|b| b.1)
}

/// Decide the purchase list.
pub fn plan_purchase(
    state: &GameState,
    me: Power,
    style: Style,
    intel: &Intel,
) -> (Action, BuyNote) {
    let map = MapData::get();
    let treasury = state.treasury[me];
    let mut note = BuyNote::default();
    if treasury < 3 {
        return (Action::EndPhase, note);
    }
    let opts = purchase_options(state);
    let cost = |k: UnitType| opts.iter().find(|o| o.unit == k).map_or(99, |o| o.cost);
    let mut left = treasury;
    let mut items: Vec<(UnitType, u32)> = vec![];
    let add = |items: &mut Vec<(UnitType, u32)>, k: UnitType, left: &mut u32| {
        match items.iter_mut().find(|i| i.0 == k) {
            Some(i) => i.1 += 1,
            None => items.push((k, 1)),
        }
        *left -= cost(k);
    };

    let complexes: Vec<LocationId> = map
        .territories()
        .filter(|&t| usable_complex(state, me, t))
        .collect();
    let land_cap: u32 = complexes
        .iter()
        .map(|&t| complex_capacity_left(state, me, t).min(1000))
        .sum();
    let ports_ok = complexes.iter().any(|&t| {
        map.adjacent_seas(t)
            .any(|z| intel.threat[z.idx()] < 3.0 && !state.hostile_sea(me, z))
    });

    // Capital danger?
    let cap = me.capital();
    let cap_threat = intel.threat[cap.idx()];
    let defensive = state.control[cap] == Some(me) && cap_threat > intel.defense[cap.idx()] * 1.3;
    note.defensive = defensive;

    // Industrial complex.
    let max_cx = opts
        .iter()
        .find(|o| o.unit == UnitType::IndustrialComplex)
        .map_or(0, |o| o.max_more);
    if max_cx > 0
        && !defensive
        && treasury >= 28
        && state.round >= 2
        && complexes.len() < 4
        && !state.settings.original_complexes_only
    {
        if let Some(site) = complex_site(state, me, intel) {
            add(&mut items, UnitType::IndustrialComplex, &mut left);
            note.complex_at = Some(site);
        }
    }

    // AA gun at a complex lacking one when enemy bombers are around.
    let enemy_bombers: Vec<LocationId> = state
        .units
        .iter()
        .filter(|u| u.kind == UnitType::Bomber && !u.owner.is_ally(me))
        .map(|u| u.location)
        .collect();
    if !enemy_bombers.is_empty() && left >= 20 {
        let lacks = complexes.iter().any(|&t| {
            !state.units_at(t).any(|u| u.kind == UnitType::AaGun)
                && enemy_bombers
                    .iter()
                    .any(|&b| bfs(state, b, 6, false, false).iter().any(|&(l, _)| l == t))
        });
        if lacks {
            add(&mut items, UnitType::AaGun, &mut left);
        }
    }

    // Weighted wish list.
    let rich = left >= 30;
    let aircraft = count(state, me, UnitType::Fighter) + count(state, me, UnitType::Bomber);
    let carriers = count(state, me, UnitType::Carrier);
    let fighters = count(state, me, UnitType::Fighter);
    let transports = count(state, me, UnitType::Transport);
    let stuck_early = state
        .units
        .iter()
        .filter(|u| {
            u.owner == me
                && matches!(u.kind, UnitType::Infantry | UnitType::Armor)
                && u.loaded_on.is_none()
                && intel.grad[u.location.idx()] >= 900.0
        })
        .count();
    let stuck = stuck_early;
    let mut bbs = 0.0;
    let (mut inf, mut arm, mut ftr, mut bmb, mut trn, mut car): (f64, f64, f64, f64, f64, f64) =
        match me {
            Power::Ussr => (5.0, 2.0, 0.4, 0.0, 0.0, 0.0),
            Power::Germany => (3.0, 2.0, 0.8, 0.0, 0.0, 0.0),
            Power::Uk => (3.0, 1.0, 0.8, 0.0, 1.0, 0.3),
            Power::Japan => (3.0, 1.0, 1.2, 0.0, 1.5, 0.4),
            Power::Usa => (3.0, 1.0, 1.0, 0.6, 1.5, 0.3),
        };
    match style {
        Style::Aggressive => {
            arm *= 1.5;
            bmb *= 1.4;
        }
        Style::Cautious => inf *= 1.4,
        Style::Balanced => {}
    }
    if !rich {
        ftr *= 0.2;
        bmb = 0.0;
        car = 0.0;
    }
    if left < 24 {
        ftr = 0.0;
    }
    if left < 35 {
        bmb = 0.0;
    }
    if aircraft >= if stuck >= 10 { 16 } else { 9 } {
        ftr = 0.0;
        bmb = 0.0;
    }
    if !ports_ok || transports >= if stuck_early >= 6 { 8 } else { 4 } || state.round > 25 {
        trn = 0.0;
        car = 0.0;
    }
    if carriers * 2 > fighters {
        ftr += 1.2;
    }
    if carriers >= 2 {
        car = 0.0;
    }
    if stuck >= 10 && ports_ok && rich {
        inf *= 0.1;
        arm *= 0.1;
        ftr = ftr.max(1.0) * 1.5;
        bmb = bmb.max(0.8) * 1.5;
        car = if carriers < 3 { 0.5 } else { 0.0 };
        bbs = 1.0;
    }
    if stuck >= 6 && trn > 0.0 {
        trn *= 2.0 + stuck as f64 / 8.0;
        inf *= 0.4;
        arm *= 0.4;
    }
    if defensive {
        inf *= 3.0;
        arm *= 1.5;
        ftr = 0.0;
        bmb = 0.0;
        trn = 0.0;
        car = 0.0;
    }
    let wish = [
        (UnitType::Infantry, inf),
        (UnitType::Armor, arm),
        (UnitType::Fighter, ftr),
        (UnitType::Bomber, bmb),
        (UnitType::Transport, trn),
        (UnitType::Carrier, car),
        (UnitType::Battleship, bbs),
    ];
    let mut land_units = 0u32;
    let mut ships_bought = 0u32;
    for _ in 0..80 {
        let mut best: Option<(f64, UnitType)> = None;
        for &(k, w) in &wish {
            if w <= 0.0 || cost(k) > left {
                continue;
            }
            if k == UnitType::Transport && ships_bought >= if stuck >= 6 { 4 } else { 2 } {
                continue;
            }
            if matches!(k, UnitType::Carrier | UnitType::Battleship) && ships_bought >= 2 {
                continue;
            }
            if !k.is_ship() && land_units >= land_cap {
                continue;
            }
            let have = items.iter().find(|i| i.0 == k).map_or(0, |i| i.1);
            let score = (f64::from(have) + 0.5) / w;
            if best.is_none_or(|b| score < b.0) {
                best = Some((score, k));
            }
        }
        let Some((_, k)) = best else { break };
        if k.is_ship() {
            ships_bought += 1;
        } else {
            land_units += 1;
        }
        add(&mut items, k, &mut left);
    }
    if items.is_empty() {
        return (Action::EndPhase, note);
    }
    note.spent = treasury - left;
    note.items = items.clone();
    let items = items
        .into_iter()
        .map(|(unit, count)| PurchaseItem { unit, count })
        .collect();
    (Action::Purchase { items }, note)
}
