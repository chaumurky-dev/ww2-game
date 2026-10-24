use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonaStyle {
    Aggressive,
    Balanced,
    Cautious,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonaVerbosity {
    Chatty,
    Normal,
    Quiet,
}

/// A selectable computer-player personality (`GET /api/bots/personas`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotPersona {
    /// Stable id stored per seat, e.g. `brass`.
    pub id: String,
    pub name: String,
    pub style: PersonaStyle,
    pub verbosity: PersonaVerbosity,
    pub blurb: String,
}
