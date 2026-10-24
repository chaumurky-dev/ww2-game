//! Income, production, capital capture, liberation, purchasing and placement.
use crate::map::MapData;
use crate::power::Power;
use crate::state::GameState;

/// Summed IPC value of the territories `p` controls, regardless of capital status.
pub fn production_level(state: &GameState, p: Power) -> u32 {
    let map = MapData::get();
    map.territories()
        .filter(|&t| state.control[t] == Some(p))
        .map(|t| map.ipc(t))
        .sum()
}

/// IPC `p` would collect now: its production level, or 0 if its capital is not in its own hands.
pub fn income(state: &GameState, p: Power) -> u32 {
    if state.control[p.capital()] == Some(p) {
        production_level(state, p)
    } else {
        0
    }
}

use crate::event::Event;
use crate::map::LocationId;
use crate::units::UnitType;

/// Is the capital of `p` currently held by an enemy of `p`?
pub fn capital_held_by_enemy(state: &GameState, p: Power) -> bool {
    matches!(state.control[p.capital()], Some(c) if !c.is_ally(p))
}

/// Transfer ownership of structures/AA guns in a territory.
fn transfer_fixtures(state: &mut GameState, loc: LocationId, new: Power, freeze: bool) {
    for u in state.units.iter_mut().filter(|u| u.location == loc) {
        if matches!(u.kind, UnitType::AaGun | UnitType::IndustrialComplex) && u.owner != new {
            u.owner = new;
            if freeze {
                u.frozen = true;
            }
        }
    }
}

/// Territory `loc` is taken by power `p`: flips control (handling liberation), moves the AA gun
/// and complex, seizes a capital's treasury, and restores territories when a capital is freed.
pub fn capture_territory(state: &mut GameState, p: Power, loc: LocationId, ev: &mut Vec<Event>) {
    let map = MapData::get();
    let prev = state.control[loc];
    let orig = map.original_owner(loc);
    let mut new = p;
    let mut liberated = false;
    if let Some(o) = orig {
        if o != p && o.is_ally(p) {
            let is_cap = map.capital_of(loc) == Some(o);
            if is_cap || !capital_held_by_enemy(state, o) {
                new = o;
                liberated = true;
            }
        }
    }
    state.control[loc] = Some(new);
    transfer_fixtures(state, loc, new, true);
    ev.push(Event::TerritoryCaptured {
        power: new,
        location: loc,
        from: prev,
        liberated,
    });
    if let Some(c) = map.capital_of(loc) {
        if new != c && !p.is_ally(c) {
            let seized = state.treasury[c];
            state.treasury[c] = 0;
            state.treasury[p] += seized;
            ev.push(Event::CapitalCaptured {
                power: p,
                capital_of: c,
                ipc_seized: seized,
            });
        } else if new == c {
            // Capital freed: territories of `c` held by allies revert to `c`.
            let mut back = vec![];
            for t in map.territories() {
                if t != loc && map.original_owner(t) == Some(c) {
                    if let Some(h) = state.control[t] {
                        if h != c && h.is_ally(c) {
                            state.control[t] = Some(c);
                            transfer_fixtures(state, t, c, false);
                            back.push(t);
                        }
                    }
                }
            }
            if !back.is_empty() {
                ev.push(Event::ControlRestored {
                    power: c,
                    locations: back,
                });
            }
        }
    }
}

use crate::error::{illegal, Result, RuleError};
use crate::state::{Phase, PurchaseItem};
use crate::tech::unit_cost;
use crate::units::{UnitId, UnitRef};

/// Maximum number of newly bought complexes in a whole game.
pub const NEW_COMPLEX_CAP: u32 = 4;

/// Total price of a purchase list for `p`.
pub fn purchase_cost(state: &GameState, p: Power, items: &[PurchaseItem]) -> u32 {
    items
        .iter()
        .map(|i| unit_cost(state, p, i.unit) * i.count)
        .sum()
}

/// Set the purchase list (replacing any earlier one this turn).
pub fn purchase(
    state: &mut GameState,
    p: Power,
    items: &[PurchaseItem],
    ev: &mut Vec<Event>,
) -> Result<()> {
    if state.phase != Phase::Purchase {
        return illegal("purchasing happens in the Purchase phase");
    }
    let mut merged: Vec<PurchaseItem> = vec![];
    for it in items.iter().filter(|i| i.count > 0) {
        match merged.iter_mut().find(|m| m.unit == it.unit) {
            Some(m) => m.count += it.count,
            None => merged.push(*it),
        }
    }
    let refund = purchase_cost(state, p, &state.purchases);
    let cost = purchase_cost(state, p, &merged);
    let have = state.treasury[p] + refund;
    if cost > have {
        return Err(RuleError::InsufficientFunds { need: cost, have });
    }
    let complexes = merged
        .iter()
        .find(|m| m.unit == UnitType::IndustrialComplex)
        .map_or(0, |m| m.count);
    if complexes > 0 && state.settings.original_complexes_only {
        return illegal("new industrial complexes are disabled in this game");
    }
    if state.new_complexes_placed + complexes > NEW_COMPLEX_CAP {
        return illegal("only four new industrial complexes can ever be bought");
    }
    state.treasury[p] = have - cost;
    state.purchases = merged.clone();
    ev.push(Event::Purchased {
        power: p,
        items: merged,
        cost,
    });
    Ok(())
}

/// Is there a complex at `loc` that `p` may produce from this turn?
pub fn usable_complex(state: &GameState, p: Power, loc: LocationId) -> bool {
    match state.complex_at(loc) {
        Some(c) => {
            c.owner == p
                && c.born_turn != state.turn.number
                && state.control[loc] == Some(p)
                && state.turn.start_control[loc] == Some(p)
        }
        None => false,
    }
}

/// Units that may still be placed at the complex territory `loc` this turn.
pub fn complex_capacity_left(state: &GameState, p: Power, loc: LocationId) -> u32 {
    let map = MapData::get();
    let placed = state
        .turn
        .placed
        .iter()
        .find(|x| x.0 == loc)
        .map_or(0, |x| x.1);
    let original = map.loc(loc).has_original_complex && map.original_owner(loc) == Some(p);
    if original {
        return u32::MAX / 2;
    }
    map.ipc(loc).max(1).saturating_sub(placed)
}

fn left_to_place(state: &GameState, kind: UnitType) -> u32 {
    state
        .purchases
        .iter()
        .find(|i| i.unit == kind)
        .map_or(0, |i| i.count)
}

/// Which complex territory would supply `count` ships placed into sea zone `zone`?
pub fn ship_source(
    state: &GameState,
    p: Power,
    zone: LocationId,
    count: u32,
    from: Option<LocationId>,
) -> Option<LocationId> {
    let map = MapData::get();
    let mut best: Option<(u32, LocationId)> = None;
    for &t in map.neighbours(zone) {
        if map.is_sea(t) || !usable_complex(state, p, t) || from.is_some_and(|f| f != t) {
            continue;
        }
        let left = complex_capacity_left(state, p, t);
        if left >= count && best.is_none_or(|b| left > b.0) {
            best = Some((left, t));
        }
    }
    best.map(|b| b.1)
}

/// Check a placement and return the complex territory whose allowance it uses.
pub fn validate_place(
    state: &GameState,
    p: Power,
    kind: UnitType,
    loc: LocationId,
    count: u32,
    from: Option<LocationId>,
) -> Result<LocationId> {
    if state.phase != Phase::Mobilize {
        return illegal("placement happens in the Mobilize phase");
    }
    if count == 0 || count > left_to_place(state, kind) {
        return illegal("you did not buy that many");
    }
    let map = MapData::get();
    match kind {
        UnitType::IndustrialComplex => {
            if count != 1 {
                return illegal("place complexes one at a time");
            }
            if map.is_sea(loc)
                || state.control[loc] != Some(p)
                || state.turn.start_control[loc] != Some(p)
            {
                return illegal("a new complex needs a territory you have controlled since the start of the turn");
            }
            if state.complex_at(loc).is_some() {
                return illegal("that territory already has a complex");
            }
            if state.new_complexes_placed >= NEW_COMPLEX_CAP {
                return illegal("the supply of new complexes is exhausted");
            }
            Ok(loc)
        }
        UnitType::Infantry
        | UnitType::Armor
        | UnitType::Fighter
        | UnitType::Bomber
        | UnitType::AaGun => {
            if map.is_sea(loc) || !usable_complex(state, p, loc) {
                return illegal("land and air units are placed at a complex you controlled at the start of the turn");
            }
            if complex_capacity_left(state, p, loc) < count {
                return illegal("that complex cannot produce so many units this turn");
            }
            if kind == UnitType::AaGun
                && (count != 1 || state.units_at(loc).any(|u| u.kind == UnitType::AaGun))
            {
                return illegal("only one AA gun per territory");
            }
            Ok(loc)
        }
        _ => {
            if !map.is_sea(loc) {
                return illegal("ships are placed in a sea zone");
            }
            if state
                .units_at(loc)
                .any(|u| !u.owner.is_ally(p) && (u.kind.is_ship() || u.kind.is_air()))
            {
                return illegal("that sea zone holds enemy units");
            }
            ship_source(state, p, loc, count, from)
                .ok_or_else(|| RuleError::illegal("no adjacent complex can supply those ships"))
        }
    }
}

/// Place purchased units.
pub fn place(
    state: &mut GameState,
    p: Power,
    kind: UnitType,
    loc: LocationId,
    count: u32,
    from: Option<LocationId>,
    ev: &mut Vec<Event>,
) -> Result<()> {
    let source = validate_place(state, p, kind, loc, count, from)?;
    let mut ids: Vec<UnitId> = vec![];
    for _ in 0..count {
        ids.push(state.spawn(kind, p, loc));
    }
    match state.turn.placed.iter_mut().find(|x| x.0 == source) {
        Some(x) => x.1 += count,
        None => state.turn.placed.push((source, count)),
    }
    if kind == UnitType::IndustrialComplex {
        state.new_complexes_placed += 1;
    }
    if let Some(it) = state.purchases.iter_mut().find(|i| i.unit == kind) {
        it.count -= count;
    }
    state.purchases.retain(|i| i.count > 0);
    ev.push(Event::UnitsPlaced {
        power: p,
        kind,
        location: loc,
        ids,
    });
    Ok(())
}

/// Throw away unplaced purchases.
pub fn discard_unplaced(state: &mut GameState, p: Power, ev: &mut Vec<Event>) {
    if state.purchases.is_empty() {
        return;
    }
    let lost: Vec<UnitRef> = state
        .purchases
        .iter()
        .flat_map(|i| {
            (0..i.count).map(move |_| UnitRef {
                id: 0,
                kind: i.unit,
                owner: p,
            })
        })
        .collect();
    state.purchases.clear();
    ev.push(Event::UnitsLost {
        power: p,
        units: lost,
        reason: "could not be placed".into(),
    });
}

/// Collect income at the end of the turn.
pub fn collect_income(state: &mut GameState, p: Power, ev: &mut Vec<Event>) {
    let amount = income(state, p);
    state.treasury[p] += amount;
    ev.push(Event::IncomeCollected {
        power: p,
        amount,
        treasury: state.treasury[p],
    });
}
