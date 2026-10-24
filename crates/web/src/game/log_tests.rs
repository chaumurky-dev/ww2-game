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
    let lines = format_events(&ev, 1);
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
    let lines = format_events(&ev, 1);
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
    let lines = format_events(&ev, 1);
    assert_eq!(lines[0].round, 2);
    assert_eq!(
        lines[1].text,
        "Soviet Union purchases 8 infantry for 24 IPC"
    );
    assert!(lines[2].text.contains("collects 24 IPC"));
}

#[test]
fn log_is_capped() {
    let mut v = vec![];
    for n in 0..250 {
        push_lines(
            &mut v,
            vec![LogLine {
                round: 1,
                power: None,
                text: n.to_string(),
            }],
        );
    }
    assert_eq!(v.len(), MAX_LINES);
    assert_eq!(v.last().unwrap().text, "249");
}
