//! UI-side domain helpers layered on top of `wartide-protocol` types (names, colours, phases,
//! unit kinds). The engine crate will eventually own unit/phase tables; until its state view lands
//! these small tables live here.
pub use wartide_protocol::game::{Power, Team};

pub trait PowerExt {
    /// CSS value (a custom property reference) holding the power colour.
    fn color(self) -> String;
}

impl PowerExt for Power {
    fn color(self) -> String {
        format!("var(--{})", self.as_str())
    }
}

pub fn team_name(t: Team) -> &'static str {
    match t {
        Team::Axis => "Axis",
        Team::Allies => "Allies",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitKind {
    Infantry,
    Armor,
    Fighter,
    Bomber,
    Battleship,
    Carrier,
    Transport,
    Submarine,
    AaGun,
    IndustrialComplex,
}

impl UnitKind {
    pub const ALL: [UnitKind; 10] = [
        UnitKind::Infantry,
        UnitKind::Armor,
        UnitKind::Fighter,
        UnitKind::Bomber,
        UnitKind::Battleship,
        UnitKind::Carrier,
        UnitKind::Transport,
        UnitKind::Submarine,
        UnitKind::AaGun,
        UnitKind::IndustrialComplex,
    ];
    /// Id used by the engine data files and the icon sprite (`unit-<id>`).
    pub fn id(self) -> &'static str {
        match self {
            UnitKind::Infantry => "infantry",
            UnitKind::Armor => "armor",
            UnitKind::Fighter => "fighter",
            UnitKind::Bomber => "bomber",
            UnitKind::Battleship => "battleship",
            UnitKind::Carrier => "carrier",
            UnitKind::Transport => "transport",
            UnitKind::Submarine => "submarine",
            UnitKind::AaGun => "aa_gun",
            UnitKind::IndustrialComplex => "industrial_complex",
        }
    }
    pub fn from_id(s: &str) -> Option<UnitKind> {
        UnitKind::ALL.into_iter().find(|k| k.id() == s)
    }
    pub fn name(self) -> &'static str {
        match self {
            UnitKind::Infantry => "Infantry",
            UnitKind::Armor => "Armor",
            UnitKind::Fighter => "Fighter",
            UnitKind::Bomber => "Bomber",
            UnitKind::Battleship => "Battleship",
            UnitKind::Carrier => "Carrier",
            UnitKind::Transport => "Transport",
            UnitKind::Submarine => "Submarine",
            UnitKind::AaGun => "AA Gun",
            UnitKind::IndustrialComplex => "Industrial Complex",
        }
    }
    /// Display-only costs; real values come from the engine's unit table.
    pub fn cost(self) -> u32 {
        match self {
            UnitKind::Infantry => 3,
            UnitKind::Armor => 5,
            UnitKind::Fighter => 12,
            UnitKind::Bomber => 15,
            UnitKind::Battleship => 24,
            UnitKind::Carrier => 18,
            UnitKind::Transport => 8,
            UnitKind::Submarine => 8,
            UnitKind::AaGun => 5,
            UnitKind::IndustrialComplex => 15,
        }
    }
}

// ---- helpers over protocol game types ----------------------------------------------------

use wartide_protocol::game::{GameMode, GameStatus, GameSummary, SeatAssignment};

/// Game title: the server-provided `name`, with a derived fallback for older rows.
pub fn game_title(g: &GameSummary) -> String {
    if !g.name.trim().is_empty() {
        return g.name.clone();
    }
    let id = g.id.to_string();
    let kind = match g.kind {
        wartide_protocol::game::GameKind::Public => "Public",
        wartide_protocol::game::GameKind::Private => "Private",
    };
    format!("{kind} game {}", &id[..4])
}

pub fn mode_label(m: GameMode) -> &'static str {
    match m {
        GameMode::TwoPlayer => "2-player",
        GameMode::FivePlayer => "5-player",
        GameMode::Custom => "custom",
    }
}

pub fn status_label(s: GameStatus) -> &'static str {
    match s {
        GameStatus::Lobby => "Waiting for players",
        GameStatus::InProgress => "In progress",
        GameStatus::Finished => "Finished",
        GameStatus::Abandoned => "Abandoned",
    }
}

pub fn my_seat(g: &GameSummary) -> Option<&SeatAssignment> {
    let i = g.my_seat?;
    g.seats.iter().find(|s| s.seat == i)
}

/// True when the current power belongs to the caller's seat.
pub fn my_turn(g: &GameSummary) -> bool {
    g.status == GameStatus::InProgress
        && match (my_seat(g), g.current_power) {
            (Some(s), Some(p)) => s.powers.contains(&p),
            _ => false,
        }
}

/// Turn deadline in epoch ms; None unless a human is on the clock (bot turns are "due" at once).
pub fn deadline_ms(g: &GameSummary) -> Option<f64> {
    if g.status != GameStatus::InProgress || waiting_on_bot(g) {
        return None;
    }
    g.turn_deadline.map(|d| d.timestamp_millis() as f64)
}

pub fn seat_label(s: &SeatAssignment) -> String {
    if s.resigned && s.is_bot {
        let who = s
            .user
            .as_ref()
            .map(|u| u.display_name.as_str())
            .unwrap_or("player");
        let bot = s
            .bot_name
            .as_deref()
            .or(s.bot_persona.as_deref())
            .unwrap_or("bot");
        format!("{who} (left, {bot} plays)")
    } else if s.is_bot {
        match s.bot_name.as_ref().or(s.bot_persona.as_ref()) {
            Some(p) => format!("bot ({p})"),
            None => "bot".into(),
        }
    } else if let Some(u) = &s.user {
        u.display_name.clone()
    } else {
        "open".into()
    }
}

/// Short note for seats that were abandoned or timed out ("resigned - bot plays", "missed 2 turns").
pub fn seat_flags(s: &SeatAssignment) -> Vec<String> {
    let mut v = vec![];
    if s.resigned {
        v.push("resigned".to_string());
    }
    if s.timeouts > 0 {
        v.push(format!(
            "{} missed turn{}",
            s.timeouts,
            if s.timeouts == 1 { "" } else { "s" }
        ));
    }
    v
}

/// Who the game is waiting for when that differs from the power whose turn it is
/// ("waiting for Germany's casualty choice").
pub fn pending_note(g: &GameSummary) -> Option<String> {
    if g.status != GameStatus::InProgress {
        return None;
    }
    match (g.pending_power, g.current_power) {
        (Some(p), Some(c)) if p != c => Some(format!(
            "Waiting for {}'s {}",
            p.name(),
            match g.phase.as_deref() {
                Some("combat") | None => "casualty choice",
                _ => "decision",
            }
        )),
        _ => None,
    }
}

/// True when the caller's seat must act now (turn or a defender decision).
pub fn my_pending(g: &GameSummary) -> bool {
    g.status == GameStatus::InProgress
        && match (my_seat(g), g.pending_power.or(g.current_power)) {
            (Some(s), Some(p)) => s.powers.contains(&p),
            _ => false,
        }
}

/// Power the game currently waits for (pending decision, else the current power).
pub fn waiting_for(g: &GameSummary) -> Option<Power> {
    g.pending_power.or(g.current_power)
}

/// True when a bot seat plays `waiting_for` (so no human deadline applies).
pub fn waiting_on_bot(g: &GameSummary) -> bool {
    waiting_for(g)
        .and_then(|p| g.seats.iter().find(|s| s.powers.contains(&p)))
        .is_some_and(|s| s.is_bot)
}

/// "Germany + Japan" for a seat's powers.
pub fn powers_text(powers: &[Power]) -> String {
    powers
        .iter()
        .map(|p| p.name())
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Seat name followed by its powers when it controls several ("Alice (UK + USA)").
pub fn seat_label_with_powers(s: &SeatAssignment) -> String {
    if s.powers.len() > 1 {
        format!("{} ({})", seat_label(s), powers_text(&s.powers))
    } else {
        seat_label(s)
    }
}

/// Banner for games that wait for somebody else: open seats in the lobby ("Waiting for a friend
/// to join as Germany + Japan"), or the other human whose powers are up ("Waiting for Alice
/// (USSR + UK)"). None when it is the caller's move or a bot plays.
pub fn waiting_banner(g: &GameSummary) -> Option<String> {
    match g.status {
        GameStatus::Lobby => {
            let open: Vec<String> = g
                .seats
                .iter()
                .filter(|s| s.user.is_none() && !s.is_bot)
                .map(|s| powers_text(&s.powers))
                .collect();
            (!open.is_empty())
                .then(|| format!("Waiting for a friend to join as {}", open.join(" / ")))
        }
        GameStatus::InProgress => {
            let p = waiting_for(g)?;
            let seat = g.seats.iter().find(|s| s.powers.contains(&p))?;
            if seat.is_bot || Some(seat.seat) == g.my_seat {
                return None;
            }
            Some(format!(
                "Waiting for {} ({})",
                seat_label(seat),
                powers_text(&seat.powers)
            ))
        }
        _ => None,
    }
}
