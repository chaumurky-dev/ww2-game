//! Log text for battle-related events (split from `log.rs`).
use super::conv::*;
use super::log::{hits_str, refs};
use wartide_engine::event::BattleOutcome;
use wartide_engine::{Event, Power};

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
