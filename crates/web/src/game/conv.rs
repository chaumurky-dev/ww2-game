//! Conversions between engine types and the UI-side types (`model::Power`, `UnitKind`), plus
//! display names. Pure; used by the model, log and panels.
use crate::model::UnitKind;
use wartide_engine::{LocationId, MapData, Phase, UnitType};

pub fn ui_kind(k: UnitType) -> UnitKind {
    match k {
        UnitType::Infantry => UnitKind::Infantry,
        UnitType::Armor => UnitKind::Armor,
        UnitType::Fighter => UnitKind::Fighter,
        UnitType::Bomber => UnitKind::Bomber,
        UnitType::Battleship => UnitKind::Battleship,
        UnitType::Carrier => UnitKind::Carrier,
        UnitType::Transport => UnitKind::Transport,
        UnitType::Submarine => UnitKind::Submarine,
        UnitType::AaGun => UnitKind::AaGun,
        UnitType::IndustrialComplex => UnitKind::IndustrialComplex,
    }
}

/// Display name of a location (territory or sea zone).
pub fn loc_name(l: LocationId) -> &'static str {
    MapData::get().loc(l).name.as_str()
}

/// Key used by the board / geometry for a location.
pub fn loc_key(l: LocationId) -> String {
    l.id().to_string()
}

pub fn unit_name(k: UnitType, n: u32) -> &'static str {
    match (k, n == 1) {
        (UnitType::Infantry, _) => "infantry",
        (UnitType::Armor, _) => "armor",
        (UnitType::Fighter, true) => "fighter",
        (UnitType::Fighter, false) => "fighters",
        (UnitType::Bomber, true) => "bomber",
        (UnitType::Bomber, false) => "bombers",
        (UnitType::Battleship, true) => "battleship",
        (UnitType::Battleship, false) => "battleships",
        (UnitType::Carrier, true) => "carrier",
        (UnitType::Carrier, false) => "carriers",
        (UnitType::Transport, true) => "transport",
        (UnitType::Transport, false) => "transports",
        (UnitType::Submarine, true) => "submarine",
        (UnitType::Submarine, false) => "submarines",
        (UnitType::AaGun, true) => "AA gun",
        (UnitType::AaGun, false) => "AA guns",
        (UnitType::IndustrialComplex, true) => "industrial complex",
        (UnitType::IndustrialComplex, false) => "industrial complexes",
    }
}

/// "3 infantry, 1 armor" for (kind, count) pairs, ordered by unit type.
pub fn describe_counts(mut v: Vec<(UnitType, u32)>) -> String {
    v.sort_by_key(|x| x.0);
    let mut merged: Vec<(UnitType, u32)> = vec![];
    for (k, n) in v {
        match merged.last_mut() {
            Some(l) if l.0 == k => l.1 += n,
            _ => merged.push((k, n)),
        }
    }
    merged
        .into_iter()
        .map(|(k, n)| format!("{n} {}", unit_name(k, n)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn phase_label(p: Phase) -> &'static str {
    match p {
        Phase::DevelopWeapons => "Research",
        Phase::Purchase => "Purchase",
        Phase::CombatMove => "Combat Move",
        Phase::Combat => "Combat",
        Phase::NonCombatMove => "Non-Combat",
        Phase::Mobilize => "Place",
        Phase::CollectIncome => "Income",
    }
}

pub const PHASES: [Phase; 7] = [
    Phase::DevelopWeapons,
    Phase::Purchase,
    Phase::CombatMove,
    Phase::Combat,
    Phase::NonCombatMove,
    Phase::Mobilize,
    Phase::CollectIncome,
];

pub fn tech_name(t: wartide_engine::Tech) -> &'static str {
    use wartide_engine::Tech::*;
    match t {
        JetPower => "Jet Power",
        Rockets => "Rockets",
        SuperSubmarines => "Super Submarines",
        LongRangeAircraft => "Long-Range Aircraft",
        IndustrialTechnology => "Industrial Technology",
        HeavyBombers => "Heavy Bombers",
    }
}

pub fn tech_short(t: wartide_engine::Tech) -> &'static str {
    use wartide_engine::Tech::*;
    match t {
        JetPower => "JET",
        Rockets => "RKT",
        SuperSubmarines => "SSB",
        LongRangeAircraft => "LRA",
        IndustrialTechnology => "IND",
        HeavyBombers => "HVY",
    }
}
