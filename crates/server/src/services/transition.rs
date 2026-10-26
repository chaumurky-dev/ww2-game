//! Pure decision of what an applied action means for the turn clock and notifications.
//! `games::apply_locked` pattern-matches the result and performs the side effects.
use wartide_protocol::game::Team;

use crate::services::games::{seat_for_power, SeatRow};
use crate::services::rules::Pending;

/// Whether (and how) to tell the next player it is their turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Nobody to tell (bot turn).
    None,
    /// Real-time push / in-app only: the same seat keeps playing another power.
    InApp,
    /// Push plus an e-mail: a different seat is up.
    Email,
}

/// What the action changed about whose move it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    /// Clock and notifications are untouched (same power still to act, or a defender decision
    /// that the driver resolves within moments, or the hand-back after one).
    Same,
    /// Control passed to another power: start its clock (`bot` turns are due immediately) and
    /// send `notice`.
    HandOff { bot: bool, notice: Notice },
    /// The game ended with this result (`None` = no winner).
    GameOver(Option<Team>),
}

/// Classify the step from `before` to `after`. `power_seat` is the seat that just acted.
pub fn transition(
    before: &Pending,
    after: &Pending,
    power_seat: &SeatRow,
    seats: &[SeatRow],
) -> Transition {
    if let Some(winner) = after.game_over {
        return Transition::GameOver(winner);
    }
    let (Some(next), true) = (after.power, after.power != before.power) else {
        return Transition::Same;
    };
    let next_seat = seat_for_power(seats, next);
    let bot = next_seat.is_some_and(|s| s.is_bot);
    // A human's defender decision is resolved by the driver within moments: no clock and no
    // "your turn" mail. Handing control back after one keeps the turn owner's running clock.
    let human_defender_next = after.defender && !bot;
    let human_defender_done = before.defender && !power_seat.is_bot;
    if human_defender_next || human_defender_done {
        return Transition::Same;
    }
    let notice = if bot {
        Notice::None
    } else if next_seat.map(|s| s.seat) != Some(power_seat.seat) {
        Notice::Email
    } else {
        Notice::InApp
    };
    Transition::HandOff { bot, notice }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wartide_protocol::game::Power;

    fn seat(n: i32, powers: &[Power], is_bot: bool) -> SeatRow {
        SeatRow {
            seat: n,
            powers: powers.iter().map(|p| p.as_str().to_string()).collect(),
            user_id: None,
            is_bot,
            bot_persona: None,
            resigned: false,
            timeouts: 0,
            missed_in_row: 0,
            username: None,
            display_name: None,
            avatar: None,
        }
    }

    fn pend(power: Power, defender: bool) -> Pending {
        Pending {
            power: Some(power),
            game_over: None,
            defender,
        }
    }

    fn seats() -> Vec<SeatRow> {
        vec![
            seat(0, &[Power::Germany, Power::Japan], false),
            seat(1, &[Power::Ussr], false),
            seat(2, &[Power::Uk], true),
        ]
    }

    #[test]
    fn human_to_human_sends_email() {
        let s = seats();
        let t = transition(
            &pend(Power::Germany, false),
            &pend(Power::Ussr, false),
            &s[0],
            &s,
        );
        assert_eq!(
            t,
            Transition::HandOff {
                bot: false,
                notice: Notice::Email
            }
        );
    }

    #[test]
    fn hand_off_to_bot_is_due_immediately_without_notice() {
        let s = seats();
        let t = transition(
            &pend(Power::Ussr, false),
            &pend(Power::Uk, false),
            &s[1],
            &s,
        );
        assert_eq!(
            t,
            Transition::HandOff {
                bot: true,
                notice: Notice::None
            }
        );
    }

    #[test]
    fn hand_off_to_own_other_power_notifies_without_email() {
        let s = seats();
        let t = transition(
            &pend(Power::Germany, false),
            &pend(Power::Japan, false),
            &s[0],
            &s,
        );
        assert_eq!(
            t,
            Transition::HandOff {
                bot: false,
                notice: Notice::InApp
            }
        );
    }

    #[test]
    fn same_power_keeps_everything() {
        let s = seats();
        let p = pend(Power::Germany, false);
        assert_eq!(transition(&p, &p, &s[0], &s), Transition::Same);
    }

    #[test]
    fn human_defender_decision_pending_gets_no_clock() {
        let s = seats();
        let t = transition(
            &pend(Power::Germany, false),
            &pend(Power::Ussr, true),
            &s[0],
            &s,
        );
        assert_eq!(t, Transition::Same);
    }

    #[test]
    fn human_defender_decision_resolved_keeps_running_clock() {
        let s = seats();
        // The Soviet player (human) just resolved a defender decision; Germany is up again.
        let t = transition(
            &pend(Power::Ussr, true),
            &pend(Power::Germany, false),
            &s[1],
            &s,
        );
        assert_eq!(t, Transition::Same);
    }

    #[test]
    fn bot_defender_decision_still_hands_off() {
        let s = seats();
        let t = transition(
            &pend(Power::Germany, false),
            &pend(Power::Uk, true),
            &s[0],
            &s,
        );
        assert_eq!(
            t,
            Transition::HandOff {
                bot: true,
                notice: Notice::None
            }
        );
    }

    #[test]
    fn game_over_wins_over_everything() {
        let s = seats();
        let after = Pending {
            power: None,
            game_over: Some(Some(Team::Axis)),
            defender: false,
        };
        assert_eq!(
            transition(&pend(Power::Germany, false), &after, &s[0], &s),
            Transition::GameOver(Some(Team::Axis))
        );
        let draw = Pending {
            game_over: Some(None),
            ..after
        };
        assert_eq!(
            transition(&pend(Power::Germany, false), &draw, &s[0], &s),
            Transition::GameOver(None)
        );
    }
}
