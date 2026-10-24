//! Player actions accepted by [`crate::apply`].
use crate::map::LocationId;
use crate::state::PurchaseItem;
use crate::units::{UnitId, UnitType};
use serde::{Deserialize, Serialize};

/// Everything a player can do. Validation is phase dependent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Develop weapons: pay 5 IPC per die and roll (phase DevelopWeapons, once per turn).
    BuyResearch {
        /// Number of research dice.
        dice: u32,
    },
    /// Set the purchase list (phase Purchase). Replaces any earlier list this turn.
    Purchase {
        /// Items bought.
        items: Vec<PurchaseItem>,
    },
    /// Move a group of units along an explicit path (excluding the start). Combat and
    /// non-combat share this action; validation depends on the phase.
    Move {
        /// Units moving together (all in the same place).
        units: Vec<UnitId>,
        /// Locations entered, in order, each adjacent to the previous (start excluded).
        path: Vec<LocationId>,
    },
    /// Board land units onto a transport in an adjacent sea zone.
    Load {
        /// Land units to load.
        units: Vec<UnitId>,
        /// The transport.
        transport: UnitId,
    },
    /// Unload cargo onto an adjacent territory.
    Unload {
        /// The transport.
        transport: UnitId,
        /// Cargo to unload.
        units: Vec<UnitId>,
        /// Destination territory.
        to: LocationId,
    },
    /// End the current phase.
    EndPhase,
    /// Start resolving the battle at a location (Combat phase).
    BeginBattle {
        /// Battle site.
        location: LocationId,
    },
    /// Declare and resolve a strategic bombing raid (Combat phase, before the land battle).
    DeclareStrategicBombing {
        /// Territory with an enemy complex.
        location: LocationId,
        /// Bombers that bomb instead of fighting.
        bombers: Vec<UnitId>,
    },
    /// Choose which of your units absorb pending hits.
    SelectCasualties {
        /// Units to remove.
        units: Vec<UnitId>,
    },
    /// Attacker retreats all remaining ground/naval units to an adjacent space.
    Retreat {
        /// Retreat destination.
        to: LocationId,
    },
    /// Attacker continues to another round.
    PressAttack,
    /// Withdraw submarines to an adjacent sea zone (empty list = decline, for defenders).
    WithdrawSubs {
        /// Submarines that leave.
        subs: Vec<UnitId>,
        /// Destination sea zone.
        to: Option<LocationId>,
    },
    /// Launch a rocket from an AA gun at an enemy complex (Combat phase, rockets tech).
    FireRocket {
        /// The AA gun.
        aa_gun: UnitId,
        /// Target territory containing an enemy industrial complex.
        target: LocationId,
    },
    /// Place purchased units (Mobilize phase).
    Place {
        /// Unit type from the purchase list.
        unit: UnitType,
        /// Territory (land/air units, complexes) or adjacent sea zone (ships).
        location: LocationId,
        /// How many.
        count: u32,
        /// For ships: which complex's allowance to use (default: the one with most room).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<LocationId>,
    },
    /// The acting power leaves the game (its seat is taken over by a bot, or the team loses).
    Concede,
}
