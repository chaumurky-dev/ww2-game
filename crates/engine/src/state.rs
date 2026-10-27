//! The complete game state. Everything here is public information.
use crate::map::{LocMap, LocationId, MapData};
use crate::power::{PerPower, Power, Team};
use crate::tech::Tech;
use crate::units::{Unit, UnitId, UnitType};
use serde::{Deserialize, Serialize};

fn t() -> bool {
    true
}
fn d84() -> Option<u32> {
    Some(84)
}

/// Per-game options. Defaults follow the Classic 2nd edition rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSettings {
    /// Research phase enabled.
    #[serde(default = "t")]
    pub weapons_development: bool,
    /// Axis economic victory threshold (combined production at end of a round).
    #[serde(default = "d84")]
    pub axis_economic_victory: Option<u32>,
    /// Optional Allied economic threshold.
    #[serde(default)]
    pub allied_economic_victory: Option<u32>,
    /// USSR may not attack on its first turn.
    #[serde(default)]
    pub soviet_first_turn_restriction: bool,
    /// No new industrial complexes may be bought.
    #[serde(default)]
    pub original_complexes_only: bool,
    /// Military victory also requires the winners to hold all their own capitals.
    #[serde(default)]
    pub total_victory: bool,
    /// Balance option: Germany starts with jet power.
    #[serde(default)]
    pub germany_jet_power: bool,
    /// Balance option: Japan starts with super submarines.
    #[serde(default)]
    pub japan_super_subs: bool,
    /// Game seed (informational; per-action seeds are supplied to `apply`).
    #[serde(default)]
    pub seed: u64,
}

impl Default for GameSettings {
    fn default() -> Self {
        GameSettings {
            weapons_development: true,
            axis_economic_victory: Some(84),
            allied_economic_victory: None,
            soviet_first_turn_restriction: false,
            original_complexes_only: false,
            total_victory: false,
            germany_jet_power: false,
            japan_super_subs: false,
            seed: 0,
        }
    }
}

/// Steps of a power's turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Research.
    DevelopWeapons,
    /// Buy units.
    Purchase,
    /// Move attackers.
    CombatMove,
    /// Resolve battles.
    Combat,
    /// Reposition, land aircraft.
    NonCombatMove,
    /// Place purchases.
    Mobilize,
    /// Income (automatic).
    CollectIncome,
}

/// How the game was won.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VictoryReason {
    /// Allies hold both Axis capitals.
    AxisCapitalsHeld,
    /// Axis hold two Allied capitals.
    AlliedCapitalsHeld,
    /// Economic threshold reached.
    Economic,
    /// The other side conceded.
    Concession,
}

/// Final result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Winner {
    /// Winning team.
    pub team: Team,
    /// Reason.
    pub reason: VictoryReason,
}

/// Which side of a battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The active power's forces.
    Attacker,
    /// The other team's forces.
    Defender,
}

/// Land or sea battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BattleKind {
    /// Land territory.
    Land,
    /// Sea zone.
    Sea,
}

/// Position in the battle state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BattleStep {
    /// Beginning of a round (round 1: bombardment).
    RoundStart,
    /// Attacking submarines' first strike.
    FirstStrike,
    /// Everyone else fires.
    MainFire,
    /// Remove casualties, check end.
    Resolve,
    /// Attacker chooses retreat / press / withdraw subs.
    Choice,
    /// Defender may withdraw submarines.
    DefenderSubChoice,
}

/// Hits waiting for a casualty selection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingHits {
    /// Who suffers the hits.
    pub side: Side,
    /// Hits that may land on anything.
    pub normal: u32,
    /// Hits from submarines (cannot be assigned to aircraft).
    pub sub: u32,
    /// Casualties removed at once (first strike) rather than at end of round.
    pub immediate: bool,
}

/// A battle in progress.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battle {
    /// Battle site.
    pub location: LocationId,
    /// Land or sea.
    pub kind: BattleKind,
    /// Attacking power.
    pub attacker: Power,
    /// Attacking units (cargo excluded).
    pub attackers: Vec<UnitId>,
    /// Defending units.
    pub defenders: Vec<UnitId>,
    /// Land battle with amphibious component (no retreat).
    pub amphibious: bool,
    /// Round number (1-based).
    pub round: u32,
    /// Current step.
    pub step: BattleStep,
    /// Outstanding casualty selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingHits>,
    /// Queued second casualty selection (attacker's losses after defender chose).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued: Option<PendingHits>,
    /// Attackers marked as casualties (still fire this round).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marked_att: Vec<UnitId>,
    /// Defenders marked as casualties (still fire this round).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marked_def: Vec<UnitId>,
    /// Bombardment already done.
    #[serde(default)]
    pub bombarded: bool,
    /// Sea zones from which an amphibious assault landed (battleships there may bombard).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assault_zones: Vec<LocationId>,
}

/// Per-turn bookkeeping for the active power.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnState {
    /// Global turn counter (increments for every power's turn, from 1).
    pub number: u32,
    /// Who controlled each territory when the turn began.
    pub start_control: LocMap<Option<Power>>,
    /// Battle sites already resolved this turn.
    pub resolved: Vec<LocationId>,
    /// Research dice already bought this turn.
    pub researched: bool,
    /// Rocket already fired this turn.
    pub rocket_fired: bool,
    /// Units placed per complex territory this turn.
    pub placed: Vec<(LocationId, u32)>,
    /// Board as it stood when the current movement phase began (None outside movement phases);
    /// the basis of `UndoMoves`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_start: Option<Box<MoveSnapshot>>,
    /// Movement actions applied successfully in the current movement phase, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub move_journal: Vec<JournalEntry>,
}

/// The parts of the game a movement phase can change, saved when the phase begins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveSnapshot {
    /// All units.
    pub units: Vec<Unit>,
    /// Next unit id.
    pub next_unit_id: UnitId,
    /// Territory control (blitzes, captures and neutral violations change it).
    pub control: LocMap<Option<Power>>,
    /// Treasuries (neutral fees, seized capital treasuries).
    pub treasury: PerPower<u32>,
}

/// One movement action applied in the current movement phase.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// The `Move`, `Load` or `Unload` as applied.
    pub action: crate::action::Action,
    /// Dice were rolled for it (antiaircraft fire), so it can never be taken back.
    #[serde(default, skip_serializing_if = "is_false")]
    pub aa_fired: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// An entry of the purchase list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseItem {
    /// Unit type.
    pub unit: UnitType,
    /// How many.
    pub count: u32,
}

/// The entire game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    /// Settings.
    pub settings: GameSettings,
    /// Round number (1-based).
    pub round: u32,
    /// Power whose turn it is.
    pub current_power: Power,
    /// Current phase.
    pub phase: Phase,
    /// IPC treasuries.
    pub treasury: PerPower<u32>,
    /// Technologies owned.
    pub techs: PerPower<Vec<Tech>>,
    /// Who controls each territory (None for untouched neutrals and sea).
    pub control: LocMap<Option<Power>>,
    /// All units.
    pub units: Vec<Unit>,
    /// Next unit id.
    pub next_unit_id: UnitId,
    /// Units bought this turn awaiting placement.
    pub purchases: Vec<PurchaseItem>,
    /// Newly bought complexes placed so far in the whole game (cap 4).
    pub new_complexes_placed: u32,
    /// Per-turn bookkeeping.
    pub turn: TurnState,
    /// Battle in progress.
    pub battle: Option<Battle>,
    /// Powers that have conceded.
    #[serde(default)]
    pub conceded: Vec<Power>,
    /// Winner, once decided.
    pub winner: Option<Winner>,
}

impl GameState {
    /// Unit by id.
    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.units
            .binary_search_by_key(&id, |u| u.id)
            .ok()
            .map(|i| &self.units[i])
    }
    /// Mutable unit by id.
    pub fn unit_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        match self.units.binary_search_by_key(&id, |u| u.id) {
            Ok(i) => Some(&mut self.units[i]),
            Err(_) => None,
        }
    }
    /// Units at a location (including cargo and carried aircraft).
    pub fn units_at(&self, loc: LocationId) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(move |u| u.location == loc)
    }
    /// Units carried by a transport/carrier.
    pub fn cargo_of(&self, ship: UnitId) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(move |u| u.loaded_on == Some(ship))
    }
    /// Controller of a territory.
    pub fn controller(&self, loc: LocationId) -> Option<Power> {
        self.control[loc]
    }
    /// Is the territory controlled by `p` or one of its allies?
    pub fn is_friendly_land(&self, p: Power, loc: LocationId) -> bool {
        matches!(self.control[loc], Some(c) if c.is_ally(p))
    }
    /// Is the territory controlled by an enemy of `p`?
    pub fn is_enemy_controlled(&self, p: Power, loc: LocationId) -> bool {
        matches!(self.control[loc], Some(c) if !c.is_ally(p))
    }
    /// Untouched neutral territory?
    pub fn is_untouched_neutral(&self, loc: LocationId) -> bool {
        MapData::get().loc(loc).neutral && self.control[loc].is_none()
    }
    /// Remove a unit (and everything it carries). Returns the removed units.
    pub fn remove_unit(&mut self, id: UnitId) -> Vec<Unit> {
        let mut out = vec![];
        let carried: Vec<UnitId> = self.cargo_of(id).map(|u| u.id).collect();
        for c in carried {
            out.extend(self.remove_unit(c));
        }
        if let Ok(i) = self.units.binary_search_by_key(&id, |u| u.id) {
            out.push(self.units.remove(i));
        }
        out
    }
    /// Add a new unit and return its id.
    pub fn spawn(&mut self, kind: UnitType, owner: Power, loc: LocationId) -> UnitId {
        let id = self.next_unit_id;
        self.next_unit_id += 1;
        self.units
            .push(Unit::new(id, kind, owner, loc, self.turn.number));
        id
    }
    /// Does any enemy of `p` have a ship at the sea zone?
    pub fn hostile_sea(&self, p: Power, loc: LocationId) -> bool {
        self.units_at(loc)
            .any(|u| u.kind.is_ship() && !u.owner.is_ally(p))
    }
    /// Does an enemy of `p` have fighting units (infantry/armor/aircraft) in the territory?
    pub fn enemy_occupied_land(&self, p: Power, loc: LocationId) -> bool {
        self.units_at(loc)
            .any(|u| u.kind.is_fighting_land_unit() && !u.owner.is_ally(p))
    }
    /// Has the owner of the AA gun / complex here (if any) enemy status vs `p`?
    pub fn enemy_aa_at(&self, p: Power, loc: LocationId) -> Option<&Unit> {
        self.units_at(loc)
            .find(|u| u.kind == UnitType::AaGun && u.loaded_on.is_none() && !u.owner.is_ally(p))
    }
    /// The industrial complex unit at a territory.
    pub fn complex_at(&self, loc: LocationId) -> Option<&Unit> {
        self.units_at(loc)
            .find(|u| u.kind == UnitType::IndustrialComplex)
    }
    /// Fighters/bombers currently landed on carriers count toward capacity: free slots on a carrier.
    pub fn carrier_free_slots(&self, carrier: UnitId) -> usize {
        2usize.saturating_sub(self.cargo_of(carrier).count())
    }
    /// Is the Soviet first-turn attack restriction in force?
    pub fn soviet_restricted(&self) -> bool {
        self.settings.soviet_first_turn_restriction
            && self.current_power == Power::Ussr
            && self.turn.number == 1
    }
    /// Is the game over?
    pub fn is_over(&self) -> bool {
        self.winner.is_some()
    }
}
