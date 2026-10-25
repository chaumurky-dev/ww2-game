//! Automatic resolution of defender decisions: standing casualty and submarine preferences.
//!
//! Defenders never wait on a human. The server resolves their `SelectCasualties` and
//! `SubWithdrawal` decisions from these per-player policies.
use crate::casualties::required_casualties;
use crate::state::GameState;
use crate::units::{Unit, UnitId, UnitType};
use serde::{Deserialize, Serialize};

/// Which units a defending player prefers to lose first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CasualtyPolicy {
    /// Lowest cost first, then lowest defence value.
    #[default]
    CheapestFirst,
    /// Lowest defence value first, then lowest cost.
    KeepBestDefenders,
    /// Like cheapest first, but aircraft are lost last.
    ProtectAircraft,
    /// Infantry first, then other units by cost; armor and aircraft last.
    KeepCounterattack,
}

impl CasualtyPolicy {
    /// All policies in display order.
    pub const ALL: [CasualtyPolicy; 4] = [
        CasualtyPolicy::CheapestFirst,
        CasualtyPolicy::KeepBestDefenders,
        CasualtyPolicy::ProtectAircraft,
        CasualtyPolicy::KeepCounterattack,
    ];

    /// Stable snake_case id (matches the serde form).
    pub fn id(self) -> &'static str {
        match self {
            CasualtyPolicy::CheapestFirst => "cheapest_first",
            CasualtyPolicy::KeepBestDefenders => "keep_best_defenders",
            CasualtyPolicy::ProtectAircraft => "protect_aircraft",
            CasualtyPolicy::KeepCounterattack => "keep_counterattack",
        }
    }

    /// Short UI label.
    pub fn label(self) -> &'static str {
        match self {
            CasualtyPolicy::CheapestFirst => "Cheapest first",
            CasualtyPolicy::KeepBestDefenders => "Keep the best defenders",
            CasualtyPolicy::ProtectAircraft => "Protect aircraft",
            CasualtyPolicy::KeepCounterattack => "Keep my counterattack force",
        }
    }

    /// One-sentence explanation for the UI.
    pub fn describe(self) -> &'static str {
        match self {
            CasualtyPolicy::CheapestFirst => {
                "Lose the cheapest units first, so the fewest IPC are lost."
            }
            CasualtyPolicy::KeepBestDefenders => {
                "Lose the units that defend worst first, so the survivors hold the line."
            }
            CasualtyPolicy::ProtectAircraft => {
                "Lose the cheapest ground and sea units first and risk aircraft only when nothing else is left."
            }
            CasualtyPolicy::KeepCounterattack => {
                "Lose infantry first and keep armor and aircraft alive for your own counterattack."
            }
        }
    }

    /// Sort key: units with the smallest key are lost first.
    fn key(self, u: &Unit) -> (u32, u32, u32) {
        let s = u.kind.stats();
        let (cost, def) = (s.cost, u32::from(s.defense));
        match self {
            CasualtyPolicy::CheapestFirst => (0, cost, def),
            CasualtyPolicy::KeepBestDefenders => (0, def, cost),
            CasualtyPolicy::ProtectAircraft => (u32::from(u.kind.is_air()), cost, def),
            CasualtyPolicy::KeepCounterattack => {
                let class = match u.kind {
                    UnitType::Infantry => 0,
                    UnitType::Armor => 2,
                    k if k.is_air() => 2,
                    _ => 1,
                };
                (class, cost, def)
            }
        }
    }
}

/// What a defending player does when submarines could slip away from a battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubWithdrawalPolicy {
    /// Always stay and fight.
    Never,
    /// Withdraw only when the battle looks lost.
    #[default]
    WhenOutgunned,
    /// Withdraw whenever it is allowed.
    Always,
}

impl SubWithdrawalPolicy {
    /// All policies in display order.
    pub const ALL: [SubWithdrawalPolicy; 3] = [
        SubWithdrawalPolicy::Never,
        SubWithdrawalPolicy::WhenOutgunned,
        SubWithdrawalPolicy::Always,
    ];

    /// Stable snake_case id (matches the serde form).
    pub fn id(self) -> &'static str {
        match self {
            SubWithdrawalPolicy::Never => "never",
            SubWithdrawalPolicy::WhenOutgunned => "when_outgunned",
            SubWithdrawalPolicy::Always => "always",
        }
    }

    /// Short UI label.
    pub fn label(self) -> &'static str {
        match self {
            SubWithdrawalPolicy::Never => "Never withdraw",
            SubWithdrawalPolicy::WhenOutgunned => "Withdraw when outgunned",
            SubWithdrawalPolicy::Always => "Always withdraw",
        }
    }

    /// One-sentence explanation for the UI.
    pub fn describe(self) -> &'static str {
        match self {
            SubWithdrawalPolicy::Never => "Your submarines always stay and fight.",
            SubWithdrawalPolicy::WhenOutgunned => {
                "Your submarines slip away when the odds say the battle is lost."
            }
            SubWithdrawalPolicy::Always => {
                "Your submarines leave every battle where they are allowed to."
            }
        }
    }
}

/// Legal casualty selection for the pending `SelectCasualties` decision under `policy`.
/// `hits`/`sub_hits`/`eligible` are the fields of the decision. The result always satisfies
/// `casualties::select_casualties`: exact count, and the first `sub_hits` picks are non-aircraft.
/// (There is no two-hit ship in this ruleset, so no unit absorbs a hit for free.)
pub fn choose_casualties(
    state: &GameState,
    policy: CasualtyPolicy,
    hits: u32,
    sub_hits: u32,
    eligible: &[UnitId],
) -> Vec<UnitId> {
    let cand: Vec<UnitId> = eligible
        .iter()
        .copied()
        .filter(|&i| state.unit(i).is_some())
        .collect();
    let normal = hits.saturating_sub(sub_hits);
    let (t, ms) = required_casualties(state, &cand, normal, sub_hits);
    let mut sorted: Vec<&Unit> = cand.iter().filter_map(|&i| state.unit(i)).collect();
    sorted.sort_by_key(|u| (policy.key(u), u.id));
    let mut chosen: Vec<UnitId> = sorted
        .iter()
        .filter(|u| !u.kind.is_air())
        .take(ms)
        .map(|u| u.id)
        .collect();
    for u in &sorted {
        if chosen.len() >= t {
            break;
        }
        if !chosen.contains(&u.id) {
            chosen.push(u.id);
        }
    }
    chosen
}
