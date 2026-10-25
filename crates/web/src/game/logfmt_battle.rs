//! Log text for battle-related events (split from `log.rs`).
use super::conv::*;
use super::log::{hits_str, refs};
use std::collections::HashMap;
use wartide_engine::event::BattleOutcome;
use wartide_engine::units::UnitRef;
use wartide_engine::{Event, LocationId, Power};

/// One battle in flight.
#[derive(Clone, Debug, Default)]
struct Fight {
    attacker: Option<Power>,
    /// Verb if the territory was captured during this battle ("takes" / "liberates").
    captured: Option<&'static str>,
    att_lost: Vec<UnitRef>,
    def_lost: Vec<UnitRef>,
}

/// Per-game accumulator of battles in flight, keyed by location. Owned by the caller of
/// `format_events` (one per log stream) because a battle's events arrive in several batches.
/// Entries are created on `BattleStarted` and removed on `BattleEnded`.
#[derive(Clone, Debug, Default)]
pub struct BattleAcc {
    fights: HashMap<LocationId, Fight>,
}

impl BattleAcc {
    pub fn in_battle(&self, loc: LocationId) -> bool {
        self.fights.contains_key(&loc)
    }

    /// Feed every event (in order, before formatting it) to keep the tallies current.
    pub(super) fn observe(&mut self, e: &Event) {
        match e {
            Event::BattleStarted {
                location, attacker, ..
            } => {
                let f = Fight {
                    attacker: Some(*attacker),
                    ..Default::default()
                };
                self.fights.insert(*location, f);
            }
            Event::CasualtiesRemoved { location, units } => {
                if let Some(f) = self.fights.get_mut(location) {
                    for u in units {
                        if Some(u.owner) == f.attacker {
                            f.att_lost.push(*u);
                        } else {
                            f.def_lost.push(*u);
                        }
                    }
                }
            }
            Event::TerritoryCaptured {
                location,
                liberated,
                ..
            } => {
                if let Some(f) = self.fights.get_mut(location) {
                    f.captured = Some(if *liberated { "liberates" } else { "takes" });
                }
            }
            _ => {}
        }
    }

    /// The synthesized summary line for `BattleStarted` / `BattleEnded` (call after `observe`;
    /// `BattleEnded` removes the battle).
    pub(super) fn summary_line(&mut self, e: &Event) -> Option<String> {
        match e {
            Event::BattleStarted {
                location,
                attacker,
                attackers,
                defenders,
                amphibious,
                ..
            } => Some(format!(
                "{} attacks {}{}: {} vs {}",
                attacker.name(),
                l(*location),
                if *amphibious { " (amphibious)" } else { "" },
                refs(attackers),
                if defenders.is_empty() {
                    "no defenders".to_string()
                } else {
                    refs(defenders)
                }
            )),
            Event::BattleEnded {
                location,
                outcome: o,
            } => {
                let f = self.fights.remove(location).unwrap_or_default();
                let a = f.attacker.map_or("Attacker", |p| p.name());
                let lost = |v: &[UnitRef]| {
                    if v.is_empty() {
                        "nothing".to_string()
                    } else {
                        refs(v)
                    }
                };
                let place = l(*location);
                let head = match (o, f.captured) {
                    (BattleOutcome::AttackerWins, Some(verb)) => {
                        format!("{a} {verb} {place}; lost {}", lost(&f.att_lost))
                    }
                    (BattleOutcome::AttackerWins, None) => {
                        format!("{a} wins at {place}; lost {}", lost(&f.att_lost))
                    }
                    (BattleOutcome::DefenderWins, _) => {
                        format!("Attack on {place} repulsed; {a} lost {}", lost(&f.att_lost))
                    }
                    (BattleOutcome::Retreated, _) => {
                        format!("{a} retreats from {place}; lost {}", lost(&f.att_lost))
                    }
                    (BattleOutcome::Stalemate, _) => {
                        format!("Stalemate at {place}; {a} lost {}", lost(&f.att_lost))
                    }
                    (BattleOutcome::Withdrawn, _) => {
                        format!("{a} withdraws from {place}; lost {}", lost(&f.att_lost))
                    }
                };
                Some(format!("{head}. Defenders lost {}.", lost(&f.def_lost)))
            }
            _ => None,
        }
    }
}

fn outcome(o: BattleOutcome) -> &'static str {
    match o {
        BattleOutcome::AttackerWins => "attacker wins",
        BattleOutcome::DefenderWins => "defender holds",
        BattleOutcome::Retreated => "attacker retreats",
        BattleOutcome::Stalemate => "stalemate",
        BattleOutcome::Withdrawn => "submarines withdraw",
    }
}

fn l(x: wartide_engine::LocationId) -> &'static str {
    loc_name(x)
}

pub(super) fn battle_arm(e: &Event, who: &mut Option<Power>, text: &mut Option<String>) {
    match e {
        Event::BattleStarted {
            location,
            attacker,
            attackers,
            defenders,
            amphibious,
            ..
        } => {
            *who = Some(*attacker);
            *text = Some(format!(
                "Battle at {}{}: {} attacks with {} against {}",
                l(*location),
                if *amphibious { " (amphibious)" } else { "" },
                attacker.name(),
                refs(attackers),
                if defenders.is_empty() {
                    "no defenders".to_string()
                } else {
                    refs(defenders)
                }
            ));
        }
        Event::BattleRound { location, round } => {
            *text = Some(format!("Round {round} of the battle at {}", l(*location)));
        }
        Event::HitsScored { hits: 0, .. } => {}
        Event::HitsScored { location, by, hits } => {
            *who = Some(*by);
            *text = Some(format!(
                "{} scores {} at {}",
                by.name(),
                hits_str(*hits),
                l(*location)
            ));
        }
        Event::CasualtiesRemoved { location, units } => {
            *text = Some(format!("Casualties at {}: {}", l(*location), refs(units)));
        }
        Event::Bombardment { location, hits } => {
            *text = Some(format!(
                "Bombardment at {} scores {}",
                l(*location),
                hits_str(*hits)
            ));
        }
        Event::Retreated { units, from, to } => {
            *text = Some(format!(
                "Retreat from {} to {}: {}",
                l(*from),
                l(*to),
                refs(units)
            ));
        }
        Event::SubsWithdrew { units, from, to } => {
            *text = Some(format!(
                "{} withdraw from {} to {}",
                refs(units),
                l(*from),
                l(*to)
            ));
        }
        Event::BattleEnded {
            location,
            outcome: o,
        } => {
            *text = Some(format!("Battle at {} ends: {}", l(*location), outcome(*o)));
        }
        Event::AssaultCancelled { location } => {
            *text = Some(format!("Amphibious assault on {} cancelled", l(*location)));
        }
        Event::StrategicBombing {
            power,
            location,
            victim,
            damage,
        } => {
            *who = Some(*power);
            *text = Some(format!(
                "{} bombs the complex of {} at {}: {damage} damage",
                power.name(),
                victim.name(),
                l(*location)
            ));
        }
        Event::RocketFired {
            power,
            location,
            victim,
            damage,
        } => {
            *who = Some(*power);
            *text = Some(format!(
                "{} fires a rocket at {} ({}): {damage} damage",
                power.name(),
                l(*location),
                victim.name()
            ));
        }
        _ => {}
    }
}
