//! Assembling the create-game request: the settings section signals and the request itself.

use leptos::prelude::*;
use uuid::Uuid;
use wartide_protocol::game::{CreatePrivateGameRequest, GameSettings, SeatSpec};

/// The game settings section of the form.
#[derive(Clone, Copy)]
pub(super) struct Opts {
    pub(super) timeout: RwSignal<u32>,
    pub(super) weapons: RwSignal<bool>,
    pub(super) axis_econ: RwSignal<bool>,
    pub(super) axis_th: RwSignal<String>,
    pub(super) ally_econ: RwSignal<bool>,
    pub(super) ally_th: RwSignal<String>,
    pub(super) soviet: RwSignal<bool>,
    pub(super) orig: RwSignal<bool>,
    pub(super) total: RwSignal<bool>,
    pub(super) jet: RwSignal<bool>,
    pub(super) subs: RwSignal<bool>,
}

impl Opts {
    pub(super) fn new() -> Self {
        Opts {
            timeout: RwSignal::new(172_800),
            weapons: RwSignal::new(true),
            axis_econ: RwSignal::new(true),
            axis_th: RwSignal::new("84".into()),
            ally_econ: RwSignal::new(false),
            ally_th: RwSignal::new("84".into()),
            soviet: RwSignal::new(false),
            orig: RwSignal::new(false),
            total: RwSignal::new(false),
            jet: RwSignal::new(false),
            subs: RwSignal::new(false),
        }
    }

    pub(super) fn settings(&self) -> GameSettings {
        let threshold = |on: RwSignal<bool>, val: RwSignal<String>| {
            on.get_untracked()
                .then(|| val.get_untracked().trim().parse().unwrap_or(84))
        };
        GameSettings {
            turn_timeout_secs: self.timeout.get_untracked(),
            weapons_development: self.weapons.get_untracked(),
            axis_economic_victory: threshold(self.axis_econ, self.axis_th),
            allied_economic_victory: threshold(self.ally_econ, self.ally_th),
            soviet_first_turn_restriction: self.soviet.get_untracked(),
            original_complexes_only: self.orig.get_untracked(),
            total_victory: self.total.get_untracked(),
            germany_jet_power: self.jet.get_untracked(),
            japan_super_subs: self.subs.get_untracked(),
        }
    }
}

/// The one place the create request is assembled.
pub(super) fn build_request(
    seats: Vec<SeatSpec>,
    name: &str,
    settings: GameSettings,
    group: &str,
) -> CreatePrivateGameRequest {
    let title = name.trim().to_string();
    CreatePrivateGameRequest {
        seats,
        name: (!title.is_empty()).then_some(title),
        settings,
        group_id: group.parse::<Uuid>().ok(),
    }
}
