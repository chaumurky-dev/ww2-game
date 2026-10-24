//! Fixed catalog of computer-player personas.
use wartide_protocol::bots::{BotPersona, PersonaStyle, PersonaVerbosity};

use crate::error::{AppError, AppResult};

type Row = (
    &'static str,
    &'static str,
    PersonaStyle,
    PersonaVerbosity,
    &'static str,
);

const CATALOG: [Row; 6] = [
    (
        "brass",
        "Colonel Brass",
        PersonaStyle::Aggressive,
        PersonaVerbosity::Chatty,
        "Charges first and explains later. Loves a bold opening and says so, loudly.",
    ),
    (
        "frost",
        "General Frost",
        PersonaStyle::Cautious,
        PersonaVerbosity::Quiet,
        "Patient and unhurried. Digs in, builds up, and rarely says a word.",
    ),
    (
        "kestrel",
        "Admiral Kestrel",
        PersonaStyle::Balanced,
        PersonaVerbosity::Normal,
        "Keeps the sea lanes open and weighs every trade. A steady, fair opponent.",
    ),
    (
        "volkov",
        "Marshal Volkov",
        PersonaStyle::Aggressive,
        PersonaVerbosity::Normal,
        "Masses armor and pushes on the main front until something breaks.",
    ),
    (
        "tide",
        "Commodore Tide",
        PersonaStyle::Balanced,
        PersonaVerbosity::Chatty,
        "Friendly commentator who narrates every move and every dice roll.",
    ),
    (
        "thistle",
        "Major Thistle",
        PersonaStyle::Cautious,
        PersonaVerbosity::Chatty,
        "Careful planner who double-checks the odds out loud before committing.",
    ),
];

pub fn catalog() -> Vec<BotPersona> {
    CATALOG
        .iter()
        .map(|(id, name, style, verbosity, blurb)| BotPersona {
            id: (*id).into(),
            name: (*name).into(),
            style: *style,
            verbosity: *verbosity,
            blurb: (*blurb).into(),
        })
        .collect()
}

pub fn persona_name(id: &str) -> Option<&'static str> {
    CATALOG.iter().find(|r| r.0 == id).map(|r| r.1)
}

/// Default persona id for a seat that did not choose one.
pub fn default_for_seat(seat: usize) -> &'static str {
    CATALOG[seat % CATALOG.len()].0
}

pub fn validate_id(id: &str) -> AppResult<&'static str> {
    CATALOG
        .iter()
        .find(|r| r.0 == id)
        .map(|r| r.0)
        .ok_or_else(|| {
            AppError::BadRequest("invalid_persona", format!("unknown bot persona '{id}'"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let c = catalog();
        assert_eq!(c.len(), 6);
        let mut ids: Vec<_> = c.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 6);
        assert_eq!(persona_name("brass"), Some("Colonel Brass"));
        assert!(validate_id("nope").is_err());
        assert_eq!(default_for_seat(7), "frost");
    }
}
