//! Victory conditions (RULES 1.1).
use crate::economy::production_level;
use crate::power::{Power, Team};
use crate::state::{GameState, VictoryReason, Winner};

fn team_of_controller(state: &GameState, loc: crate::map::LocationId) -> Option<Team> {
    state.control[loc].map(|p| p.team())
}

/// Military victory, checked at the end of every turn.
pub fn check_military(state: &GameState) -> Option<Winner> {
    let held = |p: Power, by: Team| team_of_controller(state, p.capital()) == Some(by);
    let allies_win = held(Power::Germany, Team::Allies) && held(Power::Japan, Team::Allies);
    let axis_caps = [Power::Ussr, Power::Uk, Power::Usa]
        .iter()
        .filter(|&&p| held(p, Team::Axis))
        .count();
    let axis_win = axis_caps >= 2;
    let own_ok = |team: Team| team.powers().iter().all(|&p| held(p, team));
    if allies_win && (!state.settings.total_victory || own_ok(Team::Allies)) {
        return Some(Winner {
            team: Team::Allies,
            reason: VictoryReason::AxisCapitalsHeld,
        });
    }
    if axis_win && (!state.settings.total_victory || own_ok(Team::Axis)) {
        return Some(Winner {
            team: Team::Axis,
            reason: VictoryReason::AlliedCapitalsHeld,
        });
    }
    None
}

/// Economic victory, checked after the last power's turn of a round.
pub fn check_economic(state: &GameState) -> Option<Winner> {
    let sum = |team: Team| {
        team.powers()
            .iter()
            .map(|&p| production_level(state, p))
            .sum::<u32>()
    };
    if let Some(th) = state.settings.axis_economic_victory {
        if sum(Team::Axis) >= th {
            return Some(Winner {
                team: Team::Axis,
                reason: VictoryReason::Economic,
            });
        }
    }
    if let Some(th) = state.settings.allied_economic_victory {
        if sum(Team::Allies) >= th {
            return Some(Winner {
                team: Team::Allies,
                reason: VictoryReason::Economic,
            });
        }
    }
    None
}
