//! Tests for the log formatting.

use super::log::*;
use crate::model::Power as UiPower;
use wartide_engine::event::DiceContext;
use wartide_engine::state::PurchaseItem;
use wartide_engine::units::UnitRef;
use wartide_engine::{Event, LocationId, Power};
use wartide_engine::{MapData, UnitType};

fn loc(id: &str) -> LocationId {
    MapData::get().id_of(id).unwrap_or_else(|| panic!("{id}"))
}
fn uref(id: u32, kind: UnitType) -> UnitRef {
    UnitRef {
        id,
        kind,
        owner: Power::Germany,
    }
}

#[test]
fn move_line_names_units_and_places() {
    let ev = vec![Event::UnitsMoved {
        power: Power::Germany,
        units: vec![
            uref(1, UnitType::Infantry),
            uref(2, UnitType::Infantry),
            uref(3, UnitType::Infantry),
            uref(4, UnitType::Armor),
        ],
        from: loc("germany"),
        path: vec![MapData::get().neighbours(loc("germany"))[0]],
    }];
    let lines = format_events(&ev, 1, &mut BattleAcc::default());
    assert_eq!(lines.len(), 1);
    let t = &lines[0].text;
    assert!(t.starts_with("Germany moves 3 infantry, 1 armor "), "{t}");
    assert!(t.contains('\u{2192}'));
    assert_eq!(lines[0].power, Some(UiPower::Germany));
}

#[test]
fn dice_merge_with_hits() {
    let ev = vec![
        Event::DiceRolled {
            power: Power::Germany,
            context: DiceContext::Attack {
                location: loc("germany"),
                round: 1,
            },
            rolls: vec![4, 2, 6, 1],
            details: vec![],
        },
        Event::HitsScored {
            location: loc("germany"),
            by: Power::Germany,
            hits: 2,
        },
    ];
    let lines = format_events(&ev, 1, &mut BattleAcc::default());
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "Attack dice: 4 2 6 1 (2 hits)");
}

#[test]
fn purchase_income_and_turn() {
    let ev = vec![
        Event::TurnStarted {
            round: 2,
            power: Power::Ussr,
        },
        Event::Purchased {
            power: Power::Ussr,
            items: vec![PurchaseItem {
                unit: UnitType::Infantry,
                count: 8,
            }],
            cost: 24,
        },
        Event::IncomeCollected {
            power: Power::Ussr,
            amount: 24,
            treasury: 24,
        },
    ];
    let lines = format_events(&ev, 1, &mut BattleAcc::default());
    assert_eq!(lines[0].round, 2);
    assert_eq!(
        lines[1].text,
        "Soviet Union purchases 8 infantry for 24 IPC"
    );
    assert!(lines[2].text.contains("collects 24 IPC"));
}

fn line(level: LogLevel, text: String) -> LogLine {
    LogLine {
        round: 1,
        power: None,
        text,
        level,
    }
}

#[test]
fn log_is_capped_per_level() {
    let mut v = vec![line(LogLevel::Summary, "s".into())];
    for n in 0..450 {
        push_lines(&mut v, vec![line(LogLevel::Detail, n.to_string())]);
    }
    let det = v.iter().filter(|x| x.level == LogLevel::Detail).count();
    assert_eq!(det, MAX_LINES_PER_LEVEL);
    assert_eq!(v.last().unwrap().text, "449");
    assert_eq!(v[0].text, "s", "summary survives a long detailed stream");
}

fn battle_events() -> Vec<Event> {
    let k = loc("karelia_s_s_r");
    let def = UnitRef {
        id: 9,
        kind: UnitType::Infantry,
        owner: Power::Ussr,
    };
    vec![
        Event::BattleStarted {
            location: k,
            kind: wartide_engine::state::BattleKind::Land,
            attacker: Power::Germany,
            attackers: vec![uref(1, UnitType::Infantry), uref(2, UnitType::Armor)],
            defenders: vec![def],
            amphibious: false,
        },
        Event::BattleRound {
            location: k,
            round: 1,
        },
        Event::DiceRolled {
            power: Power::Germany,
            context: DiceContext::Attack {
                location: k,
                round: 1,
            },
            rolls: vec![1, 2],
            details: vec![],
        },
        Event::HitsScored {
            location: k,
            by: Power::Germany,
            hits: 1,
        },
        Event::CasualtiesRemoved {
            location: k,
            units: vec![def],
        },
        Event::CasualtiesRemoved {
            location: k,
            units: vec![uref(1, UnitType::Infantry)],
        },
        Event::TerritoryCaptured {
            power: Power::Germany,
            location: k,
            from: Some(Power::Ussr),
            liberated: false,
        },
        Event::BattleEnded {
            location: k,
            outcome: wartide_engine::event::BattleOutcome::AttackerWins,
        },
    ]
}

fn summaries(lines: &[LogLine]) -> Vec<&LogLine> {
    lines
        .iter()
        .filter(|l| l.level == LogLevel::Summary)
        .collect()
}

#[test]
fn land_battle_has_one_start_and_one_end_summary() {
    let lines = format_events(&battle_events(), 1, &mut BattleAcc::default());
    let s = summaries(&lines);
    assert_eq!(s.len(), 2, "{s:?}");
    assert_eq!(
        s[0].text,
        "Germany attacks Karelia S.S.R.: 1 infantry, 1 armor vs 1 infantry"
    );
    assert_eq!(
        s[1].text,
        "Germany takes Karelia S.S.R.; lost 1 infantry. Defenders lost 1 infantry."
    );
    assert!(lines
        .iter()
        .any(|l| l.level == LogLevel::Detail && l.text.starts_with("Attack dice")));
    assert_eq!(visible(&lines, LogLevel::Summary).len(), 2);
    assert_eq!(visible(&lines, LogLevel::Detail).len(), lines.len());
}

#[test]
fn accumulator_survives_split_batches() {
    let ev = battle_events();
    let mut acc = BattleAcc::default();
    let mut lines = format_events(&ev[..3], 1, &mut acc);
    lines.extend(format_events(&ev[3..5], 1, &mut acc));
    lines.extend(format_events(&ev[5..], 1, &mut acc));
    let whole = format_events(&ev, 1, &mut BattleAcc::default());
    assert_eq!(summaries(&lines), summaries(&whole));
}

#[test]
fn repulsed_attack_and_standalone_capture() {
    let mut ev = battle_events();
    ev.remove(6); // no capture
    *ev.last_mut().unwrap() = Event::BattleEnded {
        location: loc("karelia_s_s_r"),
        outcome: wartide_engine::event::BattleOutcome::DefenderWins,
    };
    let lines = format_events(&ev, 1, &mut BattleAcc::default());
    let s = summaries(&lines);
    assert_eq!(
        s[1].text,
        "Attack on Karelia S.S.R. repulsed; Germany lost 1 infantry. Defenders lost 1 infantry."
    );
    let cap = vec![Event::TerritoryCaptured {
        power: Power::Germany,
        location: loc("karelia_s_s_r"),
        from: Some(Power::Ussr),
        liberated: false,
    }];
    let l = format_events(&cap, 1, &mut BattleAcc::default());
    assert_eq!(l[0].level, LogLevel::Summary);
}
