//! Canned mock data. TODO(protocol): remove once `api.rs` talks to the server.
use super::types::*;
use crate::components::board::{BoardModel, Highlight, Stack};

pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

const HOUR: f64 = 3_600_000.0;

pub fn user_named(name: &str) -> User {
    User {
        username: name.to_string(),
        display_name: name.to_string(),
        email: format!("{name}@example.com"),
        bio: "Grand strategist. Sends too many bombers.".into(),
    }
}

pub fn my_games() -> Vec<GameSummary> {
    let opp = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
    vec![
        GameSummary {
            id: "g1".into(),
            name: "Friday night Fronts".into(),
            mode: "5-player",
            round: 4,
            current_power: Power::Uk,
            my_powers: vec![Power::Uk],
            deadline_ms: now_ms() + 5.0 * HOUR,
            opponents: opp(&["ana", "bruno", "bot:Cautious", "bot:Aggressive"]),
        },
        GameSummary {
            id: "g2".into(),
            name: "Public 2p #1042".into(),
            mode: "2-player",
            round: 9,
            current_power: Power::Germany,
            my_powers: vec![Power::Uk, Power::Usa, Power::Ussr],
            deadline_ms: now_ms() + 30.0 * HOUR,
            opponents: opp(&["viktor"]),
        },
        GameSummary {
            id: "g3".into(),
            name: "Office league".into(),
            mode: "5-player",
            round: 2,
            current_power: Power::Japan,
            my_powers: vec![Power::Usa],
            deadline_ms: now_ms() + 70.0 * HOUR,
            opponents: opp(&["cho", "dana", "eli", "fay"]),
        },
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameDetail {
    pub summary: GameSummary,
    pub phase: Phase,
    pub treasury: Vec<Treasury>,
    pub board: BoardModel,
    pub messages: Vec<ChatMsg>,
    pub log: Vec<LogEntry>,
}

pub fn game_detail(id: &str) -> GameDetail {
    let summary = my_games()
        .into_iter()
        .find(|g| g.id == id)
        .unwrap_or_else(|| my_games().remove(0));
    let st = |power, kind, count| Stack { power, kind, count };
    let mut board = BoardModel::default();
    for (loc, p) in [
        ("eastern_usa", Power::Usa),
        ("united_kingdom", Power::Uk),
        ("africa", Power::Uk),
        ("western_europe", Power::Germany),
        ("germany", Power::Germany),
        ("italy", Power::Germany),
        ("ussr", Power::Ussr),
        ("japan", Power::Japan),
    ] {
        board.owners.insert(loc.into(), p);
    }
    board.stacks.insert(
        "united_kingdom".into(),
        vec![
            st(Power::Uk, UnitKind::Infantry, 4),
            st(Power::Uk, UnitKind::Fighter, 2),
            st(Power::Uk, UnitKind::IndustrialComplex, 1),
        ],
    );
    board.stacks.insert(
        "germany".into(),
        vec![
            st(Power::Germany, UnitKind::Infantry, 6),
            st(Power::Germany, UnitKind::Armor, 3),
        ],
    );
    board.stacks.insert(
        "ussr".into(),
        vec![
            st(Power::Ussr, UnitKind::Infantry, 8),
            st(Power::Ussr, UnitKind::Armor, 2),
        ],
    );
    board.stacks.insert(
        "eastern_usa".into(),
        vec![
            st(Power::Usa, UnitKind::Infantry, 3),
            st(Power::Usa, UnitKind::Bomber, 1),
        ],
    );
    board.stacks.insert(
        "sz_north_atlantic".into(),
        vec![
            st(Power::Uk, UnitKind::Battleship, 1),
            st(Power::Uk, UnitKind::Transport, 1),
        ],
    );
    board.stacks.insert(
        "sz_north_pacific".into(),
        vec![
            st(Power::Japan, UnitKind::Carrier, 1),
            st(Power::Japan, UnitKind::Submarine, 2),
        ],
    );
    board.stacks.insert(
        "japan".into(),
        vec![st(Power::Japan, UnitKind::Infantry, 5)],
    );
    board
        .highlights
        .insert("western_europe".into(), Highlight::Attack);
    board
        .highlights
        .insert("sz_north_sea".into(), Highlight::Move);
    let treasury = Power::ALL
        .iter()
        .zip([24, 37, 30, 26, 41])
        .map(|(p, ipc)| Treasury { power: *p, ipc })
        .collect();
    let msg = |channel, author: &str, power, bot, text: &str| ChatMsg {
        channel,
        author: author.into(),
        power,
        bot,
        text: text.into(),
    };
    GameDetail {
        phase: if id == "g2" {
            Phase::CombatMove
        } else {
            Phase::Purchase
        },
        treasury,
        board,
        messages: vec![
            msg(
                Channel::All,
                "ana",
                Some(Power::Usa),
                false,
                "Good luck everyone!",
            ),
            msg(
                Channel::Team,
                "Germany (bot)",
                Some(Power::Germany),
                true,
                "I'm massing infantry in Western Europe; Italy, can you cover the Mediterranean?",
            ),
            msg(
                Channel::Team,
                "bruno",
                Some(Power::Ussr),
                false,
                "Holding the line in the east.",
            ),
            msg(
                Channel::Power,
                "me",
                Some(Power::Uk),
                false,
                "Note to self: buy transports.",
            ),
        ],
        log: vec![
            LogEntry {
                round: 4,
                text: "Germany purchased 3 armor, 4 infantry".into(),
            },
            LogEntry {
                round: 4,
                text: "Germany attacked Western Europe: 2 hits, 1 hit".into(),
            },
            LogEntry {
                round: 4,
                text: "Germany collected 37 IPC".into(),
            },
        ],
        summary,
    }
}

pub fn profile(name: &str) -> Profile {
    Profile {
        user: user_named(name),
        ratings: vec![
            Rating {
                ladder: "Public 2-player",
                rating: 1612,
                wins: 14,
                losses: 9,
            },
            Rating {
                ladder: "Public 5-player",
                rating: 1534,
                wins: 6,
                losses: 8,
            },
            Rating {
                ladder: "Group: Coffee Club",
                rating: 1480,
                wins: 3,
                losses: 2,
            },
        ],
        recent: vec![
            RecentGame {
                id: "g2".into(),
                name: "Public 2p #1042".into(),
                result: "In progress",
                power: Power::Uk,
            },
            RecentGame {
                id: "g0".into(),
                name: "Office league".into(),
                result: "Won",
                power: Power::Usa,
            },
            RecentGame {
                id: "g9".into(),
                name: "Public 5p #77".into(),
                result: "Lost",
                power: Power::Japan,
            },
        ],
    }
}

pub fn leaderboard(ladder: &str) -> Vec<LeaderRow> {
    let n = ["viktor", "ana", "bruno", "cho", "dana", "eli", "fay", "gus"];
    let base = if ladder == "5p" { 1700 } else { 1800 };
    n.iter()
        .enumerate()
        .map(|(i, u)| LeaderRow {
            rank: i as u32 + 1,
            username: (*u).into(),
            rating: base - i as u32 * 37,
            wins: 30 - i as u32 * 3,
            losses: 5 + i as u32 * 2,
        })
        .collect()
}

pub fn groups() -> Vec<Group> {
    vec![
        Group {
            slug: "coffee-club".into(),
            name: "Coffee Club".into(),
            description: "Weekly games over coffee.".into(),
            members: vec!["ana".into(), "bruno".into(), "me".into()],
            joined: true,
        },
        Group {
            slug: "night-owls".into(),
            name: "Night Owls".into(),
            description: "Fast timers, sharp elbows.".into(),
            members: vec!["cho".into(), "dana".into()],
            joined: false,
        },
    ]
}
