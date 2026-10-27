//! Shared game-screen context: engine state, UI selection, log, dice and the send backend.
use super::dice::{dice_shows, DiceShow};
use super::log::{format_events, push_lines, BattleAcc, LogLine};
use super::moves::{group_reach, movable_locations};
use leptos::prelude::*;
use wartide_engine::query::Reach;
use wartide_engine::{
    pending_decision, Action, Decision, Event, GameState, LocationId, Phase, Power, UnitId,
    UnitType,
};

/// What the viewer has picked on the board / in the panels.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Sel {
    pub from: Option<LocationId>,
    pub units: Vec<UnitId>,
    pub dest: Option<LocationId>,
    /// Unit type being placed (Mobilize).
    pub place: Option<UnitType>,
    pub place_count: u32,
    /// Sea zone whose supplying complex still has to be chosen.
    pub place_zone: Option<LocationId>,
}

impl Sel {
    /// Drop references that no longer hold after the state changed.
    pub fn sanitized(&self, st: &GameState) -> Sel {
        let mut s = self.clone();
        s.dest = None;
        s.place_zone = None;
        s.units
            .retain(|&id| st.unit(id).is_some_and(|u| Some(u.location) == self.from));
        if s.units.is_empty() && s.from.is_some_and(|f| st.units_at(f).count() == 0) {
            s.from = None;
        }
        if let Some(k) = s.place {
            let left = st
                .purchases
                .iter()
                .find(|i| i.unit == k)
                .map_or(0, |i| i.count);
            if left == 0 || st.phase != Phase::Mobilize {
                s.place = None;
            } else {
                s.place_count = s.place_count.clamp(1, left);
            }
        }
        s
    }
}

/// Income summary shown after a collection: (power, amount, new treasury).
pub type IncomeNote = (Power, u32, u32);

#[derive(Clone, Copy)]
pub struct GameCtx {
    pub state: RwSignal<GameState>,
    /// Powers the viewer controls.
    pub viewer: Signal<Vec<Power>>,
    pub sel: RwSignal<Sel>,
    pub dice: RwSignal<Vec<DiceShow>>,
    pub log: RwSignal<Vec<LogLine>>,
    /// In-flight battle accumulator for the log (see `format_events`).
    pub battles: StoredValue<BattleAcc>,
    pub income: RwSignal<Option<IncomeNote>>,
    pub busy: RwSignal<bool>,
    /// Online games: the server resolves defenders' casualties and submarine withdrawals.
    pub auto_defense: bool,
    /// Locations to flash on the board after new events (cleared after ~1.6 s).
    pub flash: RwSignal<Vec<LocationId>>,
    /// Destinations of the picked units (movement phases).
    pub reach: Memo<Vec<Reach>>,
    /// Locations with units that can still act (movement phases, viewer's turn).
    pub movable: Memo<Vec<LocationId>>,
    backend: StoredValue<Option<Callback<Vec<Action>>>>,
    counter: StoredValue<u64>,
    seen: StoredValue<Vec<u64>>,
}

impl GameCtx {
    pub fn new(initial: GameState, viewer: Signal<Vec<Power>>) -> Self {
        let state = RwSignal::new(initial);
        let sel = RwSignal::new(Sel::default());
        let reach = Memo::new(move |_| {
            let s = sel.get();
            if s.units.is_empty() {
                return vec![];
            }
            state.with(|st| group_reach(st, &s.units))
        });
        let movable = Memo::new(move |_| {
            let v = viewer.get();
            state.with(|st| match pending_decision(st) {
                Decision::Phase {
                    power,
                    phase: Phase::CombatMove | Phase::NonCombatMove,
                } if v.contains(&power) => movable_locations(st, power),
                _ => vec![],
            })
        });
        GameCtx {
            state,
            viewer,
            sel,
            dice: RwSignal::new(vec![]),
            log: RwSignal::new(vec![]),
            battles: StoredValue::new(BattleAcc::default()),
            income: RwSignal::new(None),
            busy: RwSignal::new(false),
            auto_defense: false,
            flash: RwSignal::new(vec![]),
            reach,
            movable,
            backend: StoredValue::new(None),
            counter: StoredValue::new(1),
            seen: StoredValue::new(vec![]),
        }
    }

    /// True the first time a dice group is rendered (so its roll-in animation plays once).
    pub fn first_view(&self, id: u64) -> bool {
        let mut fresh = false;
        self.seen.update_value(|v| {
            if !v.contains(&id) {
                v.push(id);
                if v.len() > 64 {
                    v.remove(0);
                }
                fresh = true;
            }
        });
        fresh
    }

    /// Install the function that executes actions (local engine or server).
    pub fn set_backend(&self, cb: Callback<Vec<Action>>) {
        self.backend.set_value(Some(cb));
    }

    /// Send actions in order, stopping at the first rejection.
    pub fn send_all(&self, actions: Vec<Action>) {
        // Deferred so the click that triggered it finishes bubbling before the panel re-renders
        // (otherwise the same click can reach the freshly rendered button).
        if self.busy.get_untracked() {
            return; // a request is in flight: ignore double clicks / Enter repeats
        }
        if let Some(cb) = self.backend.get_value() {
            leptos::task::spawn_local(async move { cb.run(actions) });
        }
    }

    pub fn send(&self, a: Action) {
        self.send_all(vec![a]);
    }

    /// The power whose input is needed now (None when the game is over).
    pub fn acting(&self) -> Option<Power> {
        self.state.with(|st| match pending_decision(st) {
            Decision::Phase { power, .. }
            | Decision::SelectCasualties { power, .. }
            | Decision::RetreatOrPress { power, .. }
            | Decision::SubWithdrawal { power, .. } => Some(power),
            Decision::GameOver { .. } => None,
        })
    }

    pub fn controls(&self, p: Power) -> bool {
        self.viewer.with(|v| v.contains(&p))
    }

    /// Record the result of applied actions: log lines, dice, income note, new state.
    pub fn ingest(&self, before: &GameState, after: GameState, events: &[Event], animate: bool) {
        let lines = self
            .battles
            .try_update_value(|a| format_events(events, before.round, a))
            .unwrap_or_default();
        self.log.update(|v| push_lines(v, lines));
        let base = self.counter.get_value();
        self.counter.set_value(base + events.len() as u64 + 1);
        if animate {
            let mut locs = super::model::event_locations(events);
            locs.extend(super::model::undo_locations(&after, events));
            locs.sort_by_key(|l| l.0);
            locs.dedup();
            if !locs.is_empty() {
                self.flash.set(locs);
                let flash = self.flash;
                gloo_timers::callback::Timeout::new(1600, move || flash.set(vec![])).forget();
            }
            if events
                .iter()
                .any(|e| matches!(e, Event::TurnStarted { .. }))
            {
                self.dice.set(vec![]);
            }
            let shows = dice_shows(before, &after, events, base);
            if !shows.is_empty() {
                self.dice.set(shows);
            }
        }
        if let Some(Event::IncomeCollected {
            power,
            amount,
            treasury,
        }) = events
            .iter()
            .rev()
            .find(|e| matches!(e, Event::IncomeCollected { .. }))
        {
            self.income.set(Some((*power, *amount, *treasury)));
        }
        let undone = events
            .iter()
            .any(|e| matches!(e, Event::MovesUndone { .. }));
        if undone || before.phase != after.phase || before.current_power != after.current_power {
            self.sel.set(Sel::default());
        } else {
            self.sel.update(|s| *s = s.sanitized(&after));
        }
        self.state.set(after);
    }

    /// Replace the state without logging (initial load).
    pub fn reset_state(&self, st: GameState) {
        self.sel.set(Sel::default());
        self.state.set(st);
    }

    pub fn power_ui(p: Power) -> crate::model::Power {
        p
    }
}
