//! Seat-layout rules of the create-game form as plain data (no signals, no DOM), so they can be
//! unit-tested natively. The view holds one `RwSignal<SetupForm>` and calls these methods.

use crate::model::*;
use wartide_protocol::game::{
    check_partition, preset_layout, solo_seats, GameMode, SeatKind, SeatSpec, Team,
};

/// Seats the form can hold (one per power).
const MAX_SEATS: usize = 5;

/// Layout state of the create-game form.
///
/// With one human the host plays a whole side and every enemy power is a computer player;
/// with 2-5 humans it is the classic split for that many people (other seats open for friends).
/// "Custom" frees the seat count and the power split entirely.
#[derive(Clone, PartialEq, Debug)]
pub struct SetupForm {
    /// Number of human players (1-5); selects the classic preset.
    pub humans: usize,
    /// Side the host plays in a one-human game.
    pub side: Team,
    /// Custom split in effect (rather than the classic preset).
    pub custom: bool,
    /// Visible seats (1-5).
    pub seat_count: usize,
    /// Seat index of each power, in `Power::ALL` order.
    pub assign: [usize; 5],
    /// Who plays each seat; slots past `seat_count` are kept but ignored.
    pub kinds: [SeatKind; 5],
    /// Chosen persona id per seat (empty = server default); only used for bot seats.
    pub personas: [String; 5],
}

impl Default for SetupForm {
    /// One human on the Allies side, as the page opens.
    fn default() -> Self {
        let mut form = SetupForm {
            humans: 1,
            side: Team::Allies,
            custom: false,
            seat_count: 2,
            assign: assign_from_layout(&preset_layout(2)),
            kinds: [
                SeatKind::Host,
                SeatKind::Open,
                SeatKind::Open,
                SeatKind::Open,
                SeatKind::Open,
            ],
            personas: Default::default(),
        };
        form.apply_preset();
        form
    }
}

impl SetupForm {
    /// Apply the classic preset for the current number of humans (and side, for one human).
    /// Personas the user already picked are kept.
    pub fn apply_preset(&mut self) {
        self.custom = false;
        let specs: Vec<SeatSpec> = if self.humans == 1 {
            solo_seats(self.side)
        } else {
            preset_layout(self.humans)
                .into_iter()
                .enumerate()
                .map(|(i, powers)| SeatSpec {
                    powers,
                    kind: if i == 0 {
                        SeatKind::Host
                    } else {
                        SeatKind::Open
                    },
                    persona: None,
                })
                .collect()
        };
        let layout: Vec<Vec<Power>> = specs.iter().map(|s| s.powers.clone()).collect();
        self.seat_count = specs.len();
        self.assign = assign_from_layout(&layout);
        for (kind, spec) in self.kinds.iter_mut().zip(&specs) {
            *kind = spec.kind;
        }
    }

    /// Choose the number of human players and switch to its classic preset.
    pub fn set_humans(&mut self, n: usize) {
        self.humans = n;
        self.apply_preset();
    }

    /// Choose the host's side in a one-human game and re-apply the preset.
    pub fn set_side(&mut self, side: Team) {
        self.side = side;
        self.apply_preset();
    }

    /// The "Split" selector: the classic preset, or a custom split (keeping the current one).
    pub fn set_classic(&mut self, classic: bool) {
        if classic {
            self.apply_preset();
        } else {
            self.custom = true;
        }
    }

    /// Custom split: change the number of seats, keeping exactly one host among the visible ones.
    /// Powers assigned to seats that no longer exist are not moved (see [`Self::problems`]).
    pub fn set_seat_count(&mut self, n: usize) {
        self.seat_count = n;
        self.custom = true;
        if !self.visible_kinds().contains(&SeatKind::Host) {
            self.kinds[0] = SeatKind::Host;
        }
    }

    /// Custom split: give the power at `power_index` (`Power::ALL` order) to `seat`.
    pub fn assign_power(&mut self, power_index: usize, seat: usize) {
        self.assign[power_index] = seat;
        self.custom = true;
    }

    /// Set who plays `seat`; choosing the host seat demotes any other host to an open seat.
    pub fn set_kind(&mut self, seat: usize, kind: SeatKind) {
        if kind == SeatKind::Host {
            for (j, k) in self.kinds.iter_mut().enumerate() {
                if j != seat && *k == SeatKind::Host {
                    *k = SeatKind::Open;
                }
            }
        }
        self.kinds[seat] = kind;
    }

    /// Set the persona id of a seat (empty = server default).
    pub fn set_persona(&mut self, seat: usize, persona: String) {
        self.personas[seat] = persona;
    }

    /// Powers of `seat`, in turn order.
    pub fn seat_powers(&self, seat: usize) -> Vec<Power> {
        Power::ALL
            .into_iter()
            .zip(&self.assign)
            .filter(|(_, s)| **s == seat)
            .map(|(p, _)| p)
            .collect()
    }

    /// Powers of every visible seat.
    pub fn layout(&self) -> Vec<Vec<Power>> {
        (0..self.seat_count).map(|i| self.seat_powers(i)).collect()
    }

    /// Mode the server will derive from this layout.
    pub fn mode(&self) -> GameMode {
        GameMode::derive(&self.layout())
    }

    /// Why the form cannot be submitted (empty = fine). The first entry is the one to show.
    pub fn problems(&self) -> Vec<String> {
        if let Err(e) = check_partition(&self.layout()) {
            return vec![e.to_string()];
        }
        let hosts = self
            .visible_kinds()
            .iter()
            .filter(|k| **k == SeatKind::Host)
            .count();
        if hosts != 1 {
            return vec!["Exactly one seat must be yours.".to_string()];
        }
        Vec::new()
    }

    /// The seats to send to the server.
    pub fn to_seats(&self) -> Vec<SeatSpec> {
        self.layout()
            .into_iter()
            .enumerate()
            .map(|(i, powers)| {
                let kind = self.kinds[i];
                let persona = (kind == SeatKind::Bot && !self.personas[i].is_empty())
                    .then(|| self.personas[i].clone());
                SeatSpec {
                    powers,
                    kind,
                    persona,
                }
            })
            .collect()
    }

    fn visible_kinds(&self) -> &[SeatKind] {
        &self.kinds[..self.seat_count.min(MAX_SEATS)]
    }
}

fn assign_from_layout(layout: &[Vec<Power>]) -> [usize; 5] {
    let mut a = [0; 5];
    for (i, seat) in layout.iter().enumerate() {
        for p in seat {
            if let Some(k) = Power::ALL.iter().position(|x| x == p) {
                a[k] = i;
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts(f: &SetupForm) -> usize {
        f.visible_kinds()
            .iter()
            .filter(|k| **k == SeatKind::Host)
            .count()
    }

    #[test]
    fn default_form_matches_what_the_page_sent() {
        // Previously: apply_preset() with 1 human / Allies => solo_seats(Allies), kinds from the
        // spec, personas empty => None.
        let f = SetupForm::default();
        assert!(f.problems().is_empty());
        assert_eq!(f.mode(), GameMode::Custom);
        let expected = vec![
            SeatSpec {
                powers: vec![Power::Ussr, Power::Uk, Power::Usa],
                kind: SeatKind::Host,
                persona: None,
            },
            SeatSpec {
                powers: vec![Power::Germany],
                kind: SeatKind::Bot,
                persona: None,
            },
            SeatSpec {
                powers: vec![Power::Japan],
                kind: SeatKind::Bot,
                persona: None,
            },
        ];
        assert_eq!(f.to_seats(), expected);
        assert_eq!(f.to_seats(), solo_seats(Team::Allies));
        assert!(!f.custom);
    }

    #[test]
    fn presets_for_one_to_five_humans() {
        for n in 1..=5 {
            let mut f = SetupForm::default();
            f.set_humans(n);
            assert!(f.problems().is_empty(), "{n}: {:?}", f.problems());
            assert_eq!(hosts(&f), 1, "{n}");
            assert!(!f.custom);
            let seats = f.to_seats();
            let layout: Vec<Vec<Power>> = seats.iter().map(|s| s.powers.clone()).collect();
            assert!(check_partition(&layout).is_ok());
            if n == 1 {
                assert_eq!(seats.len(), 3);
                assert!(seats[1..].iter().all(|s| s.kind == SeatKind::Bot));
            } else {
                assert_eq!(seats.len(), n.max(2));
                assert_eq!(layout, preset_layout(n));
                assert_eq!(seats[0].kind, SeatKind::Host);
                assert!(seats[1..].iter().all(|s| s.kind == SeatKind::Open));
            }
        }
        let mut f = SetupForm::default();
        f.set_humans(2);
        assert_eq!(f.mode(), GameMode::TwoPlayer);
        f.set_humans(5);
        assert_eq!(f.mode(), GameMode::FivePlayer);
    }

    #[test]
    fn solo_side_swap() {
        let mut f = SetupForm::default();
        f.set_side(Team::Axis);
        let seats = f.to_seats();
        assert_eq!(seats, solo_seats(Team::Axis));
        let host = seats.iter().find(|s| s.kind == SeatKind::Host).unwrap();
        assert!(host.powers.iter().all(|p| p.team() == Team::Axis));
        assert_eq!(hosts(&f), 1);
        f.set_side(Team::Allies);
        assert_eq!(f.to_seats(), solo_seats(Team::Allies));
        // the side is ignored with several humans
        f.set_humans(3);
        f.set_side(Team::Axis);
        assert_eq!(f.humans, 3);
    }

    #[test]
    fn custom_seat_count_changes() {
        let mut f = SetupForm::default();
        f.set_humans(5);
        f.set_seat_count(4);
        assert!(f.custom);
        // the fifth power now points at a missing seat: reported, not silently fixed
        assert_eq!(f.problems().len(), 1);
        f.assign_power(4, 3);
        assert!(f.problems().is_empty());
        assert_eq!(f.to_seats().len(), 4);
        // a one-seat game holds every power
        f.set_seat_count(1);
        for k in 0..5 {
            f.assign_power(k, 0);
        }
        assert!(f.problems().is_empty());
        assert_eq!(f.to_seats()[0].powers, Power::ALL.to_vec());
        // growing leaves the new seat empty until a power is moved there
        f.set_seat_count(2);
        assert_eq!(f.problems().len(), 1);
        f.assign_power(0, 1);
        assert!(f.problems().is_empty());
        assert_eq!(f.mode(), GameMode::Custom);
    }

    #[test]
    fn host_is_unique_and_survives_seat_count_changes() {
        let mut f = SetupForm::default();
        f.set_humans(4);
        f.set_kind(2, SeatKind::Host);
        assert_eq!(hosts(&f), 1);
        assert_eq!(f.kinds[2], SeatKind::Host);
        assert_eq!(f.kinds[0], SeatKind::Open);
        // hiding the host's seat promotes seat 0 (the orphaned power is reported separately)
        f.set_seat_count(2);
        assert_eq!(f.kinds[0], SeatKind::Host);
        // no host at all is a problem
        let mut f = SetupForm::default();
        f.set_humans(2);
        f.set_kind(0, SeatKind::Open);
        assert_eq!(
            f.problems(),
            vec!["Exactly one seat must be yours.".to_string()]
        );
    }

    #[test]
    fn bots_get_personas_only_when_chosen() {
        let mut f = SetupForm::default();
        f.set_persona(1, "grumpy".into());
        f.set_persona(0, "ignored".into());
        let seats = f.to_seats();
        assert_eq!(seats[0].persona, None);
        assert_eq!(seats[1].persona.as_deref(), Some("grumpy"));
        assert_eq!(seats[2].persona, None);
        // a persona on a seat that stops being a bot is not sent
        f.set_kind(1, SeatKind::Open);
        assert_eq!(f.to_seats()[1].persona, None);
    }

    #[test]
    fn split_selector() {
        let mut f = SetupForm::default();
        f.set_humans(3);
        f.set_classic(false);
        assert!(f.custom);
        assert_eq!(f.seat_count, 3);
        f.assign_power(0, 1);
        f.set_classic(true);
        assert!(!f.custom);
        assert_eq!(f.layout(), preset_layout(3));
    }
}
